use crate::algorithm::InputSelectionAlgorithm;
use crate::algorithms::utils;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::FeeEstimator;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;
use std::collections::HashSet;

pub struct LargestFirstMultiAsset {
    available_inputs: Vec<InputBuilderResult>,
    available_indices: HashSet<usize>,
}

impl LargestFirstMultiAsset {
    #[allow(unused)]
    pub fn new(
        available_inputs: Vec<
            cardano_multiplatform_lib::builders::input_builder::InputBuilderResult,
        >,
    ) -> Self {
        let available_indices = HashSet::from_iter(0..available_inputs.len());
        Self {
            available_inputs,
            available_indices,
        }
    }
}

impl<Estimate: FeeEstimator> InputSelectionAlgorithm<Estimate> for LargestFirstMultiAsset {
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
        let mut chosen_indices = HashSet::<usize>::new();

        // run largest-fist by each asset type
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
                    let asset_chosen_indices = utils::cip2_largest_first_by(
                        estimator,
                        &self.available_inputs,
                        &mut self.available_indices,
                        &mut input_total,
                        &mut output_total,
                        &mut fee,
                        |value| {
                            value
                                .multiasset()
                                .as_ref()?
                                .get(&policy_id)?
                                .get(&asset_name)
                        },
                    )?;
                    chosen_indices.extend(asset_chosen_indices);
                }
            }
        }

        // add in remaining ADA
        let ada_chosen_indices = utils::cip2_largest_first_by(
            estimator,
            &self.available_inputs,
            &mut self.available_indices,
            &mut input_total,
            &mut output_total,
            &mut fee,
            |value| Some(value.coin()),
        )?;
        chosen_indices.extend(ada_chosen_indices);

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
            "LargestFirstMultiAsset algo can't balance change",
        ))
    }
}
