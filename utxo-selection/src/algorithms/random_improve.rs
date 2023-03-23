use crate::algorithm::InputSelectionAlgorithm;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::TransactionFeeEstimator;
use anyhow::anyhow;
use dcspark_core::tx::{TransactionAsset, UTxOBuilder, UTxODetails};
use dcspark_core::{AssetName, PolicyId, Regulated, TokenId, UTxOStore};
use deps::bigdecimal::ToPrimitive;
use rand::Rng;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub struct RandomImprove {
    available_inputs: Vec<UTxODetails>,
    available_indices: BTreeSet<usize>,
}

impl TryFrom<UTxOStore> for RandomImprove {
    type Error = anyhow::Error;

    fn try_from(value: UTxOStore) -> Result<Self, Self::Error> {
        RandomImprove::try_from(
            value
                .iter()
                .map(|(_, v)| v.as_ref().clone())
                .collect::<Vec<_>>(),
        )
    }
}

impl TryFrom<Vec<UTxODetails>> for RandomImprove {
    type Error = anyhow::Error;

    fn try_from(value: Vec<UTxODetails>) -> Result<Self, Self::Error> {
        let available_indices = BTreeSet::from_iter(0..value.len());
        Ok(Self {
            available_inputs: value,
            available_indices,
        })
    }
}

impl InputSelectionAlgorithm for RandomImprove {
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn set_available_inputs(
        &mut self,
        available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()> {
        let _available_indices = BTreeSet::from_iter(0..available_inputs.len());
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
        let mut input_balance = input_output_setup.input_balance;
        let output_balance = input_output_setup.output_balance;
        let mut fee = estimator.min_required_fee()?;

        let mut asset_input_balance = input_output_setup.input_asset_balance;
        let asset_output_balance = input_output_setup.output_asset_balance;

        let explicit_outputs = input_output_setup.fixed_outputs.clone();

        let mut chosen_indices = HashSet::<usize>::new();

        let mut rng = rand::thread_rng();
        let mut policy_ids_to_asset_names = asset_output_balance
            .values()
            .map(|asset: &TransactionAsset| (asset.policy_id.clone(), asset.asset_name.clone()))
            .collect::<Vec<(PolicyId, AssetName)>>();
        policy_ids_to_asset_names.sort_by(
            |left: &(PolicyId, AssetName), right: &(PolicyId, AssetName)| match left.0.cmp(&right.0)
            {
                Ordering::Equal => left.1.cmp(&right.1),
                x => x,
            },
        );

        for (policy_id, asset_name) in policy_ids_to_asset_names.iter() {
            let token = TokenId::new(format!("{policy_id}:{asset_name}"));
            let asset_chosen_indices = select_input_and_update_balances(
                &self.available_inputs,
                &mut self.available_indices,
                &explicit_outputs,
                estimator,
                &mut asset_input_balance,
                &mut input_balance,
                &mut fee,
                |value: &UTxODetails| {
                    value
                        .assets
                        .iter()
                        .find(|asset| asset.fingerprint == token)
                        .map(|asset| asset.quantity.clone())
                },
                |value: &UTxOBuilder| {
                    value
                        .assets
                        .iter()
                        .find(|asset| asset.fingerprint == token)
                        .map(|asset| asset.quantity.clone())
                },
                &mut rng,
            )?;

            chosen_indices.extend(asset_chosen_indices);
        }

        // add in remaining ADA
        let ada_chosen_indices = select_input_and_update_balances(
            &self.available_inputs,
            &mut self.available_indices,
            &explicit_outputs,
            estimator,
            &mut asset_input_balance,
            &mut input_balance,
            &mut fee,
            |value: &UTxODetails| Some(value.value.clone()),
            |value: &UTxOBuilder| Some(value.value.clone()),
            &mut rng,
        )?;
        chosen_indices.extend(ada_chosen_indices);

        // Phase 3: add extra inputs needed for fees (not covered by CIP-2)
        // We do this at the end because this new inputs won't be associated with
        // a specific output, so the improvement algorithm we do above does not apply here.
        while input_balance < output_balance {
            if self.available_indices.is_empty() {
                return Err(anyhow!("UTxO Balance Insufficient[x]"));
            }
            let i = *self
                .available_indices
                .iter()
                .nth(rng.gen_range(0..self.available_indices.len()))
                .unwrap();
            self.available_indices.remove(&i);
            let input = &self.available_inputs[i];
            let input_fee = estimator.fee_for_input(input)?;
            estimator.add_input(input.clone())?;
            input_balance += &input.value;
            for asset in input.assets.iter() {
                asset_input_balance
                    .entry(asset.fingerprint.clone())
                    .or_insert(TransactionAsset::new(
                        asset.policy_id.clone(),
                        asset.asset_name.clone(),
                        asset.fingerprint.clone(),
                    ))
                    .quantity += &asset.quantity;
            }
            fee += input_fee;
            chosen_indices.insert(i);
        }

        Ok(InputSelectionResult {
            fixed_inputs: input_output_setup.fixed_inputs,
            fixed_outputs: input_output_setup.fixed_outputs,
            chosen_inputs: chosen_indices
                .into_iter()
                .map(|i| self.available_inputs[i].clone())
                .collect(),
            changes: vec![],
            input_balance,
            output_balance,
            fee,

            input_asset_balance: asset_input_balance,
            output_asset_balance: asset_output_balance,
        })
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        self.available_indices
            .iter()
            .map(|index| self.available_inputs[*index].clone())
            .collect::<Vec<_>>()
    }
}

#[allow(clippy::too_many_arguments)]
fn select_input_and_update_balances<
    Estimate: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>,
    ByInput,
    ByOutput,
    R: Rng + ?Sized,
>(
    available_inputs: &[UTxODetails],
    available_indices: &mut BTreeSet<usize>,
    explicit_outputs: &[UTxOBuilder],
    estimator: &mut Estimate,
    asset_input_balance: &mut HashMap<TokenId, TransactionAsset>,
    input_total: &mut dcspark_core::Value<Regulated>,
    fee: &mut dcspark_core::Value<Regulated>,
    by_input: ByInput,
    by_output: ByOutput,
    rng: &mut R,
) -> anyhow::Result<HashSet<usize>>
where
    ByInput: Fn(&UTxODetails) -> Option<dcspark_core::Value<Regulated>>,
    ByOutput: Fn(&UTxOBuilder) -> Option<dcspark_core::Value<Regulated>>,
{
    let mut chosen_indices = HashSet::<usize>::new();

    let mut relevant_indices = available_indices
        .iter()
        .filter(|i| by_input(&available_inputs[**i]).is_some())
        .cloned()
        .collect::<Vec<usize>>();

    let mut associated_indices: BTreeMap<UTxOBuilder, Vec<usize>> = BTreeMap::new();
    let mut outputs = explicit_outputs
        .iter()
        .filter(|output| by_output(output).is_some())
        .cloned()
        .collect::<Vec<UTxOBuilder>>();
    outputs.sort_by_key(|output| by_output(output).expect("filtered above"));
    for output in outputs.iter().rev() {
        // TODO: how should we adapt this to inputs being associated when running for other assets?
        // if we do these two phases for each asset and don't take into account the other runs for other assets
        // then we over-add (and potentially fail if we don't have plenty of inputs)
        // On the other hand, the improvement phase it difficult to determine if a change is an improvement
        // if we're trying to improve for multiple assets at a time without knowing how important each input is
        // e.g. maybe we have lots of asset A but not much of B
        // For now I will just have this be entirely separate per-asset but we might want to in a later commit
        // consider the improvements separately and have it take some kind of dot product / distance for assets
        // during the improvement phase and have the improvement phase target multiple asset types at once.
        // One issue with that is how to scale in between different assets. We could maybe normalize them by
        // dividing each asset type by the sum of the required asset type in all outputs.
        // Another possibility for adapting this to multiassets is when associating an input x for asset type a
        // we try and subtract all other assets b != a from the outputs we're trying to cover.
        // It might make sense to diverge further and not consider it per-output and to instead just match against
        // the sum of all outputs as one single value.
        let mut added = dcspark_core::Value::zero();
        let needed = by_output(output)
            .ok_or_else(|| anyhow!("Transaction output proper amount is not found"))?;
        while added < needed {
            if relevant_indices.is_empty() {
                return Err(anyhow!("UTxO Balance Insufficient"));
            }
            let random_index = rng.gen_range(0..relevant_indices.len());
            let i = relevant_indices.swap_remove(random_index);
            available_indices.remove(&i);
            let input = &available_inputs[i];
            added +=
                &by_input(input).expect("do not call on asset types that aren't in the output");
            associated_indices
                .entry(output.clone())
                .or_default()
                .push(i);
        }
    }

    if !relevant_indices.is_empty() {
        // Phase 2: Improvement
        for output in outputs.iter_mut() {
            let associated = associated_indices
                .get_mut(output)
                .ok_or_else(|| anyhow!("Associated index by output key not found"))?;
            for i in associated.iter_mut() {
                let random_index = rng.gen_range(0..relevant_indices.len());
                let j: &mut usize = relevant_indices
                    .get_mut(random_index)
                    .ok_or_else(|| anyhow!("Relevant index by random index not found"))?;
                let should_improve = {
                    let input = &available_inputs[*i];
                    let new_input = &available_inputs[*j];
                    let cur = input.value.raw().to_u64().unwrap();
                    let new = new_input.value.raw().to_u64().unwrap();
                    let min = output.value.raw().to_u64().unwrap();
                    let ideal = 2 * min;
                    let max = 3 * min;
                    let move_closer =
                        (ideal as i128 - new as i128).abs() < (ideal as i128 - cur as i128).abs();
                    let not_exceed_max = new < max;

                    move_closer && not_exceed_max
                };
                if should_improve {
                    available_indices.insert(*i);
                    available_indices.remove(j);
                    std::mem::swap(i, j);
                }
            }
        }
    }

    // after finalizing the improvement we need to actually add these results to the builder
    for output in outputs.iter() {
        for i in associated_indices
            .get(output)
            .ok_or_else(|| anyhow!("Transaction output key not found"))?
            .iter()
        {
            let input = &available_inputs[*i];
            let input_fee = &estimator.fee_for_input(input)?;
            estimator.add_input(input.clone())?;
            *input_total += &input.value;
            for asset in input.assets.iter() {
                asset_input_balance
                    .entry(asset.fingerprint.clone())
                    .or_insert(TransactionAsset::new(
                        asset.policy_id.clone(),
                        asset.asset_name.clone(),
                        asset.fingerprint.clone(),
                    ))
                    .quantity += &asset.quantity;
            }
            *fee += input_fee;
            chosen_indices.insert(*i);
        }
    }

    Ok(chosen_indices)
}

#[cfg(test)]
mod tests {
    use crate::algorithms::test_utils::create_utxo;
    use crate::algorithms::RandomImprove;
    use crate::estimators::dummy_estimator::DummyFeeEstimate;
    use crate::{InputOutputSetup, InputSelectionAlgorithm};
    use dcspark_core::tx::UTxOBuilder;
    use dcspark_core::{Address, Regulated, UTxOStore, Value};

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

        let mut random_improve = RandomImprove::try_from(store).unwrap();

        let result = random_improve
            .select_inputs(
                &mut DummyFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: Default::default(),
                    input_asset_balance: Default::default(),
                    output_balance: Value::from(1),
                    output_asset_balance: Default::default(),
                    fixed_inputs: vec![],
                    fixed_outputs: vec![UTxOBuilder::new(Address::new(""), Value::from(1), vec![])],
                    change_address: None,
                },
            )
            .unwrap();

        assert_eq!(result.fee, Value::zero());
        assert!(result.output_balance <= result.input_balance);
    }
}
