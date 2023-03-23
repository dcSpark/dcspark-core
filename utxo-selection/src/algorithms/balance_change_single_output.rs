use crate::{
    calculate_asset_balance, calculate_main_token_balance, InputOutputSetup,
    InputSelectionAlgorithm, InputSelectionResult, TransactionFeeEstimator,
};
use anyhow::anyhow;
use dcspark_core::tx::{TransactionAsset, UTxOBuilder, UTxODetails};
use dcspark_core::{Balance, Regulated, Value};

#[derive(Default)]
pub struct SingleOutputChangeBalancer {
    available_inputs: Vec<UTxODetails>,
    extra: Option<String>,
}

impl SingleOutputChangeBalancer {
    pub fn set_extra(&mut self, extra: String) {
        self.extra = Some(extra);
    }
}

impl InputSelectionAlgorithm for SingleOutputChangeBalancer {
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn set_available_inputs(
        &mut self,
        available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()> {
        self.available_inputs = available_inputs;
        Ok(())
    }

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>> {
        let change_address = if let Some(address) = input_output_setup.change_address {
            address
        } else {
            return Err(anyhow!("change address is not provided"));
        };

        let asset_balances = calculate_asset_balance(
            &input_output_setup.input_asset_balance,
            &input_output_setup.output_asset_balance,
        );
        let mut change_assets = vec![];
        for (token, asset_balance) in asset_balances.into_iter() {
            match asset_balance {
                Balance::Debt(d) => {
                    return Err(anyhow!(
                        "there's lack of assets selected, can't balance change: {}",
                        d
                    ));
                }
                Balance::Balanced => {}
                Balance::Excess(excess) => {
                    let mut asset = input_output_setup
                        .input_asset_balance
                        .get(&token)
                        .ok_or_else(|| anyhow!("asset {} must be presented in the inputs", token))?
                        .clone();
                    asset.quantity = excess;
                    change_assets.push(asset)
                }
            }
        }

        let mut fee = estimator.min_required_fee()?;
        let current_balance = calculate_main_token_balance(
            &input_output_setup.input_balance,
            &input_output_setup.output_balance,
            &fee,
        );

        let value: Value<Regulated> = match current_balance {
            Balance::Debt(d) => {
                return Err(anyhow!(
                    "there's lack of main asset selected, can't balance change: {}",
                    d
                ));
            }
            Balance::Balanced => Value::zero(),
            Balance::Excess(excess) => excess,
        };

        let mut change = UTxOBuilder {
            address: change_address,
            value,
            assets: change_assets,
            extra: self.extra.clone(),
        };

        let fee_for_change = estimator.fee_for_output(&change)?;
        change.value -= &fee_for_change;
        fee += &fee_for_change;

        estimator.add_output(change.clone())?;

        let output_balance = &input_output_setup.output_balance + &change.value;
        let mut output_asset_balance = input_output_setup.output_asset_balance;
        for asset in change.assets.iter() {
            output_asset_balance
                .entry(asset.fingerprint.clone())
                .or_insert(TransactionAsset::new(
                    asset.policy_id.clone(),
                    asset.asset_name.clone(),
                    asset.fingerprint.clone(),
                ))
                .quantity += &asset.quantity;
        }
        Ok(InputSelectionResult {
            input_balance: input_output_setup.input_balance,
            input_asset_balance: input_output_setup.input_asset_balance,
            output_balance,
            output_asset_balance,
            fixed_inputs: input_output_setup.fixed_inputs,
            fixed_outputs: input_output_setup.fixed_outputs,
            chosen_inputs: vec![],
            changes: vec![change],
            fee,
        })
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        self.available_inputs.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::test_utils::{create_asset, create_utxo};
    use crate::algorithms::{LargestFirst, SingleOutputChangeBalancer};
    use crate::estimators::dummy_estimator::DummyFeeEstimate;
    use crate::{InputOutputSetup, InputSelectionAlgorithm};
    use dcspark_core::tx::UTxOBuilder;
    use dcspark_core::{Address, Regulated, TokenId, UTxOStore, Value};
    use std::collections::HashMap;

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
        store
            .insert(create_utxo(
                0,
                3,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("0".to_string(), Value::from(1))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                4,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![
                    create_asset("0".to_string(), Value::from(90)),
                    create_asset("1".to_string(), Value::from(90)),
                ],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                5,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("0".to_string(), Value::from(9))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                6,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("0".to_string(), Value::from(2))],
            ))
            .unwrap();
        store
            .insert(create_utxo(
                0,
                7,
                "0".to_string(),
                Value::<Regulated>::from(1),
                vec![create_asset("0".to_string(), Value::from(3))],
            ))
            .unwrap();
        let store = store.freeze();

        let mut largest_first = LargestFirst::try_from(store).unwrap();

        let mut output_balance = HashMap::new();
        output_balance.insert(
            TokenId::new("0"),
            create_asset("0".to_string(), Value::from(100)),
        );
        let result = largest_first
            .select_inputs(
                &mut DummyFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: Default::default(),
                    input_asset_balance: Default::default(),
                    output_balance: Value::from(24),
                    output_asset_balance: output_balance.clone(),
                    fixed_inputs: vec![],
                    fixed_outputs: vec![UTxOBuilder::new(
                        Address::new("unwrap"),
                        Value::<Regulated>::from(24),
                        output_balance.values().cloned().collect(),
                    )],
                    change_address: None,
                },
            )
            .unwrap();

        assert_eq!(result.fee, Value::zero());
        assert_eq!(result.output_balance, Value::from(24));
        assert_eq!(result.input_balance, Value::from(34));
        assert_eq!(
            result
                .input_asset_balance
                .get(&TokenId::new("0"))
                .cloned()
                .unwrap()
                .quantity,
            Value::from(102)
        );
        assert_eq!(
            result
                .output_asset_balance
                .get(&TokenId::new("0"))
                .cloned()
                .unwrap()
                .quantity,
            Value::from(100)
        );
        assert_eq!(result.chosen_inputs.len(), 5);

        let mut balance_change = SingleOutputChangeBalancer::default();

        let result = balance_change
            .select_inputs(
                &mut DummyFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: result.input_balance,
                    input_asset_balance: result.input_asset_balance,
                    output_balance: result.output_balance,
                    output_asset_balance: result.output_asset_balance,
                    fixed_inputs: result.chosen_inputs,
                    fixed_outputs: result.fixed_outputs,
                    change_address: Some(Address::new("kek")),
                },
            )
            .unwrap();

        assert_eq!(result.fee, Value::zero());
        assert_eq!(result.chosen_inputs.len(), 0);
        assert_eq!(result.fixed_inputs.len(), 5);
        assert_eq!(result.fixed_outputs.len(), 1);
        assert_eq!(result.changes.len(), 1);
        assert!(result.is_balanced());

        let change = result.changes.first().cloned().unwrap();
        assert_eq!(change.value, Value::<Regulated>::from(10));
        assert_eq!(
            change
                .assets
                .iter()
                .find(|asset| asset.fingerprint == TokenId::new("0"))
                .cloned()
                .unwrap(),
            create_asset("0".to_string(), Value::from(2))
        );
        assert_eq!(
            change
                .assets
                .iter()
                .find(|asset| asset.fingerprint == TokenId::new("1"))
                .cloned()
                .unwrap(),
            create_asset("1".to_string(), Value::from(90))
        );
    }
}
