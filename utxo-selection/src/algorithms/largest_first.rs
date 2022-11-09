use crate::algorithm::InputSelectionAlgorithm;
use crate::algorithms::utils;
use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::TransactionFeeEstimator;
use anyhow::anyhow;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::TransactionOutput;
use dcspark_core::Balance;
use std::collections::HashSet;

pub struct LargestFirst {
    available_inputs: Vec<InputBuilderResult>,
    available_indices: HashSet<usize>,
}

impl LargestFirst {
    #[allow(unused)]
    fn new(available_inputs: Vec<InputBuilderResult>) -> Self {
        let available_indices = HashSet::from_iter(0..available_inputs.len());
        Self {
            available_inputs,
            available_indices,
        }
    }
}

impl InputSelectionAlgorithm for LargestFirst {
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
        if !input_output_setup.output_asset_balance.is_empty()
            || !input_output_setup.input_asset_balance.is_empty()
        {
            return Err(anyhow!("Multiasset values not supported by LargestFirst. Please use LargestFirstMultiAsset"));
        }
        let mut input_total = cardano_multiplatform_lib::ledger::common::value::Value::new(
            &cardano_utils::conversion::value_to_csl_coin(&input_output_setup.input_balance)?,
        );
        let mut output_total = cardano_multiplatform_lib::ledger::common::value::Value::new(
            &cardano_utils::conversion::value_to_csl_coin(&input_output_setup.output_balance)?,
        );
        let mut fee = cardano_utils::conversion::value_to_csl_coin(&estimator.min_required_fee()?)?;

        let chosen_indices = utils::cip2_largest_first_by(
            estimator,
            &self.available_inputs,
            &mut self.available_indices,
            &mut input_total,
            &mut output_total,
            &mut fee,
            |value| Some(value.coin()),
        )?;

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
        self.available_indices
            .iter()
            .map(|index| self.available_inputs[*index].clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::LargestFirst;
    use crate::{DummyCmlFeeEstimate, InputOutputSetup, InputSelectionAlgorithm};
    use cardano_multiplatform_lib::address::Address;
    use cardano_multiplatform_lib::builders::input_builder::SingleInputBuilder;
    use cardano_multiplatform_lib::crypto::TransactionHash;
    use cardano_multiplatform_lib::ledger::common::value::{BigNum, Coin, Value};
    use cardano_multiplatform_lib::{TransactionInput, TransactionOutput};
    use dcspark_core::Regulated;

    #[test]
    fn try_select_dummy_fee() {
        let mut largest_first = LargestFirst::new(vec![]);
        let input_builder_result = SingleInputBuilder::new(
            &TransactionInput::new(
                &TransactionHash::from_hex(
                    "a90a895d07049afc725a0d6a38c6b82218b8d1de60e7bd70ecdd58f1d9e1218b",
                )
                .unwrap(),
                &BigNum::zero(),
            ),
            &TransactionOutput::new(
                &Address::from_bech32("addr1u8pcjgmx7962w6hey5hhsd502araxp26kdtgagakhaqtq8sxy9w7g")
                    .unwrap(),
                &Value::new(&Coin::zero()),
            ),
        );
        largest_first
            .set_available_inputs(vec![input_builder_result.payment_key().unwrap()])
            .unwrap();
        largest_first
            .select_inputs(&mut DummyCmlFeeEstimate::new(), InputOutputSetup::default())
            .unwrap();
    }

    #[test]
    fn try_select_dummy_fee_non_zero() {
        let mut largest_first = LargestFirst::new(vec![]);
        let input_builder_result_1 = SingleInputBuilder::new(
            &TransactionInput::new(
                &TransactionHash::from_hex(
                    "a90a895d07049afc725a0d6a38c6b82218b8d1de60e7bd70ecdd58f1d9e1218b",
                )
                .unwrap(),
                &BigNum::from(0),
            ),
            &TransactionOutput::new(
                &Address::from_bech32("addr1u8pcjgmx7962w6hey5hhsd502araxp26kdtgagakhaqtq8sxy9w7g")
                    .unwrap(),
                &Value::new(&Coin::from(1000)),
            ),
        )
        .payment_key()
        .unwrap();

        let input_builder_result_2 = SingleInputBuilder::new(
            &TransactionInput::new(
                &TransactionHash::from_hex(
                    "b90a895d07049afc725a0d6a38c6b82218b8d1de60e7bd70ecdd58f1d9e1218b",
                )
                .unwrap(),
                &BigNum::from(0),
            ),
            &TransactionOutput::new(
                &Address::from_bech32("addr1u8pcjgmx7962w6hey5hhsd502araxp26kdtgagakhaqtq8sxy9w7g")
                    .unwrap(),
                &Value::new(&Coin::from(2000)),
            ),
        )
        .payment_key()
        .unwrap();
        largest_first
            .set_available_inputs(vec![input_builder_result_1, input_builder_result_2.clone()])
            .unwrap();
        let result = largest_first
            .select_inputs(
                &mut DummyCmlFeeEstimate::new(),
                InputOutputSetup {
                    input_balance: Default::default(),
                    input_asset_balance: Default::default(),
                    output_balance: dcspark_core::Value::<Regulated>::from(200),
                    output_asset_balance: Default::default(),
                    fixed_inputs: vec![],
                    fixed_outputs: vec![],
                    change_address: None,
                },
            )
            .unwrap();
        let chosen_inputs = result.chosen_inputs;
        assert_eq!(chosen_inputs.len(), 1);
        assert_eq!(
            chosen_inputs.first().cloned().unwrap().utxo_info.amount(),
            input_builder_result_2.utxo_info.amount()
        );
    }
}
