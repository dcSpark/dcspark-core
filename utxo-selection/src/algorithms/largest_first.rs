use crate::algorithm::InputSelectionAlgorithm;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::TransactionFeeEstimator;
use crate::{calculate_main_token_balance, UTxOStoreSupport};
use anyhow::anyhow;
use dcspark_core::tx::{TransactionAsset, UTxOBuilder, UTxODetails};
use dcspark_core::{Regulated, TokenId, UTxOStore};
use std::collections::HashMap;

pub struct LargestFirst {
    available_inputs: UTxOStore,
}

impl TryFrom<UTxOStore> for LargestFirst {
    type Error = anyhow::Error;

    fn try_from(value: UTxOStore) -> Result<Self, Self::Error> {
        Ok(Self {
            available_inputs: value,
        })
    }
}

impl TryFrom<Vec<UTxODetails>> for LargestFirst {
    type Error = anyhow::Error;

    fn try_from(value: Vec<UTxODetails>) -> Result<Self, Self::Error> {
        let mut store = UTxOStore::new().thaw();
        for val in value {
            store.insert(val)?;
        }
        Ok(Self {
            available_inputs: store.freeze(),
        })
    }
}

impl UTxOStoreSupport for LargestFirst {
    fn set_available_utxos(&mut self, utxos: UTxOStore) -> anyhow::Result<()> {
        self.available_inputs = utxos;
        Ok(())
    }

    fn get_available_utxos(&mut self) -> anyhow::Result<UTxOStore> {
        Ok(self.available_inputs.clone())
    }
}

impl InputSelectionAlgorithm for LargestFirst {
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn set_available_inputs(
        &mut self,
        available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()> {
        let mut utxo_store = UTxOStore::new().thaw();
        for input in available_inputs.into_iter() {
            utxo_store.insert(input)?;
        }
        self.available_inputs = utxo_store.freeze();
        Ok(())
    }

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>> {
        let mut input_balance = input_output_setup.input_balance;
        let output_balance = input_output_setup.output_balance;
        let mut fee = estimator.min_required_fee()?;

        let mut asset_input_balance = input_output_setup.input_asset_balance;
        let asset_output_balance = input_output_setup.output_asset_balance;

        let mut selected_inputs: Vec<UTxODetails> = vec![];

        let mut utxos = self.available_inputs.clone();

        for (token, token_output_balance) in asset_output_balance.iter() {
            let mut token_input_balance = asset_input_balance
                .entry(token.clone())
                .or_insert(TransactionAsset::new(
                    token_output_balance.policy_id.clone(),
                    token_output_balance.asset_name.clone(),
                    token_output_balance.fingerprint.clone(),
                ))
                .quantity
                .clone();

            while token_input_balance < token_output_balance.quantity {
                let (new_selected_inputs, new_utxos) = select_input_and_update_balances(
                    token,
                    utxos.clone(),
                    estimator,
                    &mut asset_input_balance,
                    &mut token_input_balance,
                    &mut input_balance,
                    &mut fee,
                )?;
                selected_inputs.extend(new_selected_inputs);
                utxos = new_utxos;
            }
        }

        while calculate_main_token_balance(&input_balance, &output_balance, &fee).in_debt() {
            let (new_selected_inputs, new_utxos) = select_input_and_update_balances_for_main(
                utxos.clone(),
                estimator,
                &mut asset_input_balance,
                &mut input_balance,
                &mut fee,
            )?;
            selected_inputs.extend(new_selected_inputs);
            utxos = new_utxos;
        }

        self.available_inputs = utxos;

        Ok(InputSelectionResult {
            fixed_inputs: input_output_setup.fixed_inputs,
            fixed_outputs: input_output_setup.fixed_outputs,
            chosen_inputs: selected_inputs,
            changes: vec![],
            input_balance,
            output_balance,
            fee,

            input_asset_balance: asset_input_balance,
            output_asset_balance: asset_output_balance,
        })
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        self.available_inputs
            .iter()
            .map(|(_, v)| v.as_ref().clone())
            .collect::<Vec<_>>()
    }
}

fn select_input_and_update_balances<
    Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>,
>(
    token: &TokenId,
    utxos: UTxOStore,
    estimator: &mut Estimate,
    asset_input_balance: &mut HashMap<TokenId, TransactionAsset>,
    input_token_balance: &mut dcspark_core::Value<Regulated>,
    input_total: &mut dcspark_core::Value<Regulated>,
    fee: &mut dcspark_core::Value<Regulated>,
) -> anyhow::Result<(Vec<UTxODetails>, UTxOStore)> {
    let mut selected_inputs: Vec<UTxODetails> = vec![];

    let (selected, new_utxos) = select_largest_input_for(utxos, token)?;

    *input_total += &selected.value;
    for asset in selected.assets.iter() {
        if token != &TokenId::MAIN && &asset.fingerprint == token {
            *input_token_balance += &asset.quantity;
        }

        let current_input_asset = asset_input_balance
            .entry(asset.fingerprint.clone())
            .or_insert(TransactionAsset::new(
                asset.policy_id.clone(),
                asset.asset_name.clone(),
                asset.fingerprint.clone(),
            ));
        current_input_asset.quantity += &asset.quantity;
    }

    *fee += estimator.fee_for_input(&selected)?;
    selected_inputs.push(selected.clone());
    estimator.add_input(selected)?;

    Ok((selected_inputs, new_utxos))
}

fn select_input_and_update_balances_for_main<
    Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>,
>(
    utxos: UTxOStore,
    estimator: &mut Estimate,
    asset_input_balance: &mut HashMap<TokenId, TransactionAsset>,
    input_total: &mut dcspark_core::Value<Regulated>,
    fee: &mut dcspark_core::Value<Regulated>,
) -> anyhow::Result<(Vec<UTxODetails>, UTxOStore)> {
    let mut selected_inputs: Vec<UTxODetails> = vec![];

    let (selected, new_utxos) = select_largest_input_for(utxos, &TokenId::MAIN)?;

    *input_total += &selected.value;
    for asset in selected.assets.iter() {
        let current_input_asset = asset_input_balance
            .entry(asset.fingerprint.clone())
            .or_insert(TransactionAsset::new(
                asset.policy_id.clone(),
                asset.asset_name.clone(),
                asset.fingerprint.clone(),
            ));
        current_input_asset.quantity += &asset.quantity;
    }

    *fee += estimator.fee_for_input(&selected)?;
    selected_inputs.push(selected.clone());
    estimator.add_input(selected)?;

    Ok((selected_inputs, new_utxos))
}

pub fn select_largest_input_for(
    utxos: UTxOStore,
    asset: &TokenId,
) -> anyhow::Result<(UTxODetails, UTxOStore)> {
    let utxo = utxos
        // here we take the largest available UTxO for this given
        // asset.
        .iter_token_ordered_by_value_rev(asset)
        .next()
        .cloned()
        .ok_or_else(|| anyhow!("No more input to select for {asset}"))?;

    let mut utxos = utxos.thaw();
    utxos.remove(&utxo.pointer)?;
    Ok((utxo, utxos.freeze()))
}

#[cfg(test)]
mod tests {
    use crate::algorithms::LargestFirst;
    use crate::estimators::dummy_estimator::DummyFeeEstimate;
    use crate::{InputOutputSetup, InputSelectionAlgorithm};
    use dcspark_core::tx::{TransactionAsset, TransactionId, UTxODetails, UtxoPointer};
    use dcspark_core::{
        Address, AssetName, OutputIndex, PolicyId, Regulated, TokenId, UTxOStore, Value,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    pub fn create_utxo(
        tx: u64,
        index: u64,
        address: String,
        value: Value<Regulated>,
        assets: Vec<TransactionAsset>,
    ) -> UTxODetails {
        UTxODetails {
            pointer: UtxoPointer {
                transaction_id: TransactionId::new(tx.to_string()),
                output_index: OutputIndex::new(index),
            },
            address: Address::new(address),
            value,
            assets,
            metadata: Arc::new(Default::default()),
            extra: None,
        }
    }

    pub fn create_asset(fingerprint: String, quantity: Value<Regulated>) -> TransactionAsset {
        let fingerprint = TokenId::new(fingerprint);
        TransactionAsset {
            policy_id: PolicyId::new(fingerprint.as_ref().to_string()),
            asset_name: AssetName::new(fingerprint.as_ref().to_string()),
            fingerprint,
            quantity,
        }
    }

    #[test]
    fn try_select_dummy_fee() {
        let mut store = UTxOStore::new().thaw();
        store
            .insert(create_utxo(
                0,
                0,
                "0".to_string(),
                Value::<Regulated>::from(10),
                vec![],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                1,
                "0".to_string(),
                Value::<Regulated>::from(20),
                vec![],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                2,
                "0".to_string(),
                Value::<Regulated>::from(11),
                vec![],
            ))
            .unwrap();
        let store = store.freeze();

        let mut largest_first = LargestFirst::try_from(store).unwrap();

        let result = largest_first
            .select_inputs(
                &mut DummyFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: Default::default(),
                    input_asset_balance: Default::default(),
                    output_balance: Value::from(1),
                    output_asset_balance: Default::default(),
                    fixed_inputs: vec![],
                    fixed_outputs: vec![],
                    change_address: None,
                },
            )
            .unwrap();

        assert_eq!(result.fee, Value::zero());
        assert_eq!(result.output_balance, Value::from(1));
        assert_eq!(result.input_balance, Value::from(20));
        assert_eq!(result.chosen_inputs.len(), 1);
        assert_eq!(
            result.chosen_inputs.first().unwrap().pointer.output_index,
            OutputIndex::new(1)
        );
    }

    #[test]
    fn try_select_dummy_fee_assets() {
        let mut store = UTxOStore::new().thaw();
        store
            .insert(create_utxo(
                0,
                0,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("kek".to_string(), Value::from(1))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                1,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("kek".to_string(), Value::from(100))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                2,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("kek".to_string(), Value::from(201))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                3,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![
                    create_asset("kek".to_string(), Value::from(200)),
                    create_asset("lol".to_string(), Value::from(1)),
                ],
            ))
            .unwrap();

        store
            .insert(create_utxo(
                1,
                0,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("lol".to_string(), Value::from(1))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                1,
                1,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("lol".to_string(), Value::from(100))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                1,
                2,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("lol".to_string(), Value::from(201))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                1,
                3,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![
                    create_asset("lol".to_string(), Value::from(200)),
                    create_asset("kek".to_string(), Value::from(1)),
                ],
            ))
            .unwrap();

        store
            .insert(create_utxo(
                2,
                1,
                "0".to_string(),
                Value::<Regulated>::from(20),
                vec![],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                2,
                2,
                "0".to_string(),
                Value::<Regulated>::from(11),
                vec![],
            ))
            .unwrap();
        let store = store.freeze();

        let mut largest_first = LargestFirst::try_from(store).unwrap();

        let mut output_asset_balance = HashMap::new();
        output_asset_balance.insert(
            TokenId::new("kek"),
            create_asset("kek".to_string(), Value::from(402)),
        );
        output_asset_balance.insert(
            TokenId::new("lol"),
            create_asset("lol".to_string(), Value::from(402)),
        );

        let result = largest_first
            .select_inputs(
                &mut DummyFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: Default::default(),
                    input_asset_balance: Default::default(),
                    output_balance: Value::from(24),
                    output_asset_balance,
                    fixed_inputs: vec![],
                    fixed_outputs: vec![],
                    change_address: None,
                },
            )
            .unwrap();

        assert_eq!(result.fee, Value::zero());
        assert!(result.input_balance >= result.output_balance);
        assert!(result
            .input_asset_balance
            .values()
            .any(|asset: &TransactionAsset| asset.quantity == Value::from(402)));
        assert!(result
            .input_asset_balance
            .values()
            .any(|asset: &TransactionAsset| asset.quantity == Value::from(502)));
    }
}
