use crate::algorithm::InputSelectionAlgorithm;
use crate::algorithms::utils;
use crate::algorithms::utils::result_from_cml;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::TransactionFeeEstimator;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::TransactionOutput;
use cardano_utils::conversion::multiasset_iter;
use std::collections::HashSet;

pub struct LargestFirstMultiAsset {
    available_inputs: Vec<InputBuilderResult>,
    available_indices: HashSet<usize>,
}

impl LargestFirstMultiAsset {
    #[allow(unused)]
    fn new(available_inputs: Vec<InputBuilderResult>) -> Self {
        let available_indices = HashSet::from_iter(0..available_inputs.len());
        Self {
            available_inputs,
            available_indices,
        }
    }
}

impl InputSelectionAlgorithm for LargestFirstMultiAsset {
    type InputUtxo = InputBuilderResult;
    type OutputUtxo = TransactionOutput;

    fn set_available_inputs(
        &mut self,
        available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()> {
        let available_indices = HashSet::from_iter(0..available_inputs.len());
        self.available_inputs = available_inputs;
        self.available_indices = available_indices;
        Ok(())
    }

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>> {
        let mut input_total = cardano_utils::conversion::tokens_to_csl_value(
            &input_output_setup.input_balance,
            &input_output_setup.input_asset_balance,
        )?;
        let mut output_total = cardano_utils::conversion::tokens_to_csl_value(
            &input_output_setup.output_balance,
            &input_output_setup.output_asset_balance,
        )?;
        let mut fee = cardano_utils::conversion::value_to_csl_coin(&estimator.min_required_fee()?)?;

        let mut chosen_indices = HashSet::<usize>::new();

        // run largest-fist by each asset type
        multiasset_iter(&output_total.clone(), |policy_id, asset_name, _quantity| {
            let asset_chosen_indices = utils::cip2_largest_first_by(
                estimator,
                &self.available_inputs,
                &mut self.available_indices,
                &mut input_total,
                &mut output_total,
                &mut fee,
                |value| value.multiasset().as_ref()?.get(policy_id)?.get(asset_name),
            )?;
            chosen_indices.extend(asset_chosen_indices);
            Ok(())
        })?;

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

        let chosen_inputs = chosen_indices
            .into_iter()
            .map(|i| self.available_inputs[i].clone())
            .collect();

        result_from_cml(
            input_output_setup.fixed_inputs,
            input_output_setup.fixed_outputs,
            chosen_inputs,
            vec![],
            input_total,
            output_total,
            fee,
        )
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        self.available_indices
            .iter()
            .map(|index| self.available_inputs[*index].clone())
            .collect()
    }
}
