use crate::algorithm::InputSelectionAlgorithm;
use crate::algorithms::utils;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::FeeEstimator;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;
use cardano_multiplatform_lib::ledger::common::value::Value;
use rand::Rng;
use std::collections::{BTreeSet, HashSet};

pub struct RandomImproveMultiAsset {
    available_inputs: Vec<InputBuilderResult>,
    available_indices: BTreeSet<usize>,
}

impl RandomImproveMultiAsset {
    #[allow(unused)]
    pub fn new(
        available_inputs: Vec<
            cardano_multiplatform_lib::builders::input_builder::InputBuilderResult,
        >,
    ) -> Self {
        let available_indices = BTreeSet::from_iter(0..available_inputs.len());
        Self {
            available_inputs,
            available_indices,
        }
    }
}

impl<Estimate: FeeEstimator> InputSelectionAlgorithm<Estimate> for RandomImproveMultiAsset {
    fn add_available_input(&mut self, input: InputBuilderResult) -> Result<(), JsError> {
        let new_available_index = self.available_inputs.len();
        self.available_inputs.push(input);
        self.available_indices.insert(new_available_index);
        Ok(())
    }

    fn select_inputs(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup,
    ) -> Result<InputSelectionResult, JsError> {
        let mut input_total = input_output_setup.input_balance;
        let mut output_total = input_output_setup.output_balance;
        let mut fee = input_output_setup.fee;
        let explicit_outputs = input_output_setup.explicit_outputs;

        let mut chosen_indices = HashSet::<usize>::new();

        let mut rng = rand::thread_rng();
        // run random-improve by each asset type
        if let Some(ma) = output_total.multiasset() {
            let policy_ids = ma.keys();
            for policy_id_index in 0..policy_ids.len() {
                let policy_id = policy_ids.get(policy_id_index);
                let assets = if let Some(assets) = ma.get(&policy_id) {
                    assets
                } else {
                    continue;
                };
                let asset_names = assets.keys();
                for asset_name_index in 0..asset_names.len() {
                    let asset_name = asset_names.get(asset_name_index);
                    let asset_chosen_indices = utils::cip2_random_improve_by(
                        estimator,
                        &self.available_inputs,
                        &mut self.available_indices,
                        &mut input_total,
                        &mut output_total,
                        &explicit_outputs,
                        &mut fee,
                        |value| {
                            value
                                .multiasset()
                                .as_ref()?
                                .get(&policy_id)?
                                .get(&asset_name)
                        },
                        &mut rng,
                    )?;
                    chosen_indices.extend(asset_chosen_indices);
                }
            }
        }
        // add in remaining ADA
        let ada_chosen_indices = utils::cip2_random_improve_by(
            estimator,
            &self.available_inputs,
            &mut self.available_indices,
            &mut input_total,
            &mut output_total,
            &explicit_outputs,
            &mut fee,
            |value| Some(value.coin()),
            &mut rng,
        )?;
        chosen_indices.extend(ada_chosen_indices);

        // Phase 3: add extra inputs needed for fees (not covered by CIP-2)
        // We do this at the end because this new inputs won't be associated with
        // a specific output, so the improvement algorithm we do above does not apply here.
        while input_total.coin() < output_total.coin() {
            if self.available_indices.is_empty() {
                return Err(JsError::from_str("UTxO Balance Insufficient[x]"));
            }
            let i = *self
                .available_indices
                .iter()
                .nth(rng.gen_range(0..self.available_indices.len()))
                .unwrap();
            self.available_indices.remove(&i);
            let input = &self.available_inputs[i];
            let input_fee = estimator.fee_for_input(input)?;
            estimator.add_input(input).unwrap();
            input_total = input_total.checked_add(&input.utxo_info.amount())?;
            output_total = output_total.checked_add(&Value::new(&input_fee))?;
            fee = fee.checked_add(&input_fee)?;
            chosen_indices.insert(i);
        }

        Ok(InputSelectionResult {
            chosen_inputs: chosen_indices
                .into_iter()
                .map(|i| self.available_inputs[i].clone())
                .collect(),
            chosen_outputs: vec![],
            input_balance: input_total,
            output_balance: output_total,
            fee,
        })
    }

    fn can_balance_change(&self) -> bool {
        false
    }

    fn balance_change(
        &mut self,
        _estimator: &mut Estimate,
        _input_output_setup: InputOutputSetup,
    ) -> Result<InputSelectionResult, JsError> {
        Err(JsError::from_str(
            "RandomImproveMultiAsset algo can't balance change",
        ))
    }
}
