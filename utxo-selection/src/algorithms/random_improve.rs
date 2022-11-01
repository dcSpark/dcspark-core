use crate::algorithm::InputSelectionAlgorithm;
use crate::algorithms::utils;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::FeeEstimator;
use cardano_multiplatform_lib::error::JsError;
use cardano_multiplatform_lib::ledger::common::value::Value;
use rand::Rng;
use std::collections::BTreeSet;

pub struct RandomImprove {
    available_inputs: Vec<cardano_multiplatform_lib::builders::input_builder::InputBuilderResult>,
    available_indices: BTreeSet<usize>,
}

impl RandomImprove {
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

impl<Estimate: FeeEstimator> InputSelectionAlgorithm<Estimate> for RandomImprove {
    fn select_inputs(
        mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup,
    ) -> Result<InputSelectionResult, JsError> {
        if input_output_setup.output_balance.multiasset().is_some() {
            return Err(JsError::from_str("Multiasset values not supported by LargestFirst. Please use LargestFirstMultiAsset"));
        }
        let mut input_total = input_output_setup.input_balance;
        let mut output_total = input_output_setup.output_balance;
        let mut fee = input_output_setup.fee;
        let explicit_outputs = input_output_setup.explicit_outputs;

        let mut rng = rand::thread_rng();
        let mut chosen_indices = utils::cip2_random_improve_by(
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
            estimator.add_input(input)?;
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
}
