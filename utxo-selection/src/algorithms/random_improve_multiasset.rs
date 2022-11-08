use crate::algorithm::InputSelectionAlgorithm;
use crate::algorithms::utils;
use crate::algorithms::utils::result_from_cml;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::csl::CslTransactionOutput;
use crate::estimate::TransactionFeeEstimator;
use crate::UTxOBuilder;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;
use cardano_multiplatform_lib::ledger::common::value::Value;
use cardano_multiplatform_lib::TransactionOutput;
use cardano_utils::conversion::multiasset_iter;
use rand::Rng;
use std::collections::{BTreeSet, HashSet};
use anyhow::anyhow;

pub struct RandomImproveMultiAsset {
    available_inputs: Vec<InputBuilderResult>,
    available_indices: BTreeSet<usize>,
}

impl RandomImproveMultiAsset {
    #[allow(unused)]
    fn new(available_inputs: Vec<InputBuilderResult>) -> Self {
        let available_indices = BTreeSet::from_iter(0..available_inputs.len());
        Self {
            available_inputs,
            available_indices,
        }
    }
}

impl InputSelectionAlgorithm
    for RandomImproveMultiAsset
{
    type InputUtxo = InputBuilderResult;
    type OutputUtxo = TransactionOutput;

    fn set_available_inputs(
        &mut self,
        available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()> {
        let available_indices = BTreeSet::from_iter(0..available_inputs.len());
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

        let explicit_outputs: Vec<TransactionOutput> = input_output_setup
            .fixed_outputs
            .clone()
            .into_iter()
            .map(|output| {
                let output: CslTransactionOutput = output.into();
                output.inner
            })

            .collect();

        let mut chosen_indices = HashSet::<usize>::new();

        let mut rng = rand::thread_rng();

        // run random-improve by each asset type
        multiasset_iter(&output_total.clone(), |policy_id, asset_name, _quantity| {
            let asset_chosen_indices = utils::cip2_random_improve_by(
                estimator,
                &self.available_inputs,
                &mut self.available_indices,
                &mut input_total,
                &mut output_total,
                &explicit_outputs,
                &mut fee,
                |value| value.multiasset().as_ref()?.get(policy_id)?.get(asset_name),
                &mut rng,
            )?;
            chosen_indices.extend(asset_chosen_indices);
            Ok(())
        })?;

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
                return Err(anyhow!("UTxO Balance Insufficient[x]"));
            }
            let i = *self
                .available_indices
                .iter()
                .nth(rng.gen_range(0..self.available_indices.len()))
                .unwrap();
            self.available_indices.remove(&i);
            let input = &self.available_inputs[i];
            let input_fee =
                cardano_utils::conversion::value_to_csl_coin(&estimator.fee_for_input(input)?)?;
            estimator.add_input(input.clone()).unwrap();
            input_total = input_total.checked_add(&input.utxo_info.amount()).map_err(|err| anyhow!(err))?;
            output_total = output_total.checked_add(&Value::new(&input_fee)).map_err(|err| anyhow!(err))?;
            fee = fee.checked_add(&input_fee).map_err(|err| anyhow!(err))?;
            chosen_indices.insert(i);
        }

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
        self.available_indices.iter().map(|index| self.available_inputs[*index].clone()).collect()
    }
}
