use crate::algorithm::InputSelectionAlgorithm;
use crate::algorithms::utils;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::csl::CslTransactionOutput;
use crate::estimate::TransactionFeeEstimator;
use crate::UTxOBuilder;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;
use cardano_multiplatform_lib::ledger::common::value::Value;
use cardano_multiplatform_lib::TransactionOutput;
use dcspark_core::Balance;
use rand::Rng;
use std::collections::BTreeSet;
use anyhow::anyhow;

pub struct RandomImprove {
    available_inputs: Vec<InputBuilderResult>,
    available_indices: BTreeSet<usize>,
}

impl RandomImprove {
    #[allow(unused)]
    fn new(available_inputs: Vec<InputBuilderResult>) -> Self {
        let available_indices = BTreeSet::from_iter(0..available_inputs.len());
        Self {
            available_inputs,
            available_indices,
        }
    }
}

impl InputSelectionAlgorithm for RandomImprove
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
        if !input_output_setup.output_asset_balance.is_empty()
            || !input_output_setup.input_asset_balance.is_empty()
        {
            return Err(anyhow!("Multiasset values not supported by LargestFirst. Please use LargestFirstMultiAsset"));
        }
        let mut input_total = Value::new(&cardano_utils::conversion::value_to_csl_coin(
            &input_output_setup.input_balance,
        )?);
        let mut output_total = Value::new(&cardano_utils::conversion::value_to_csl_coin(
            &input_output_setup.output_balance,
        )?);
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
            estimator.add_input(input.clone())?;
            input_total = input_total.checked_add(&input.utxo_info.amount()).map_err(|err| anyhow!(err))?;
            output_total = output_total.checked_add(&Value::new(&input_fee)).map_err(|err| anyhow!(err))?;
            fee = fee.checked_add(&input_fee).map_err(|err| anyhow!(err))?;
            chosen_indices.insert(i);
        }

        let input_balance = cardano_utils::conversion::csl_coin_to_value(&input_total.coin())?;
        let output_balance = cardano_utils::conversion::csl_coin_to_value(&output_total.coin())?;
        let fee = cardano_utils::conversion::csl_coin_to_value(&fee)?;

        let mut balance = Balance::zero();
        balance += input_balance.clone() - output_balance.clone();

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

            input_asset_balance: Default::default(),
            output_asset_balance: Default::default(),

            balance,
            asset_balance: Default::default(),
        })
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        self.available_indices.iter().map(|index| self.available_inputs[*index].clone()).collect()
    }
}
