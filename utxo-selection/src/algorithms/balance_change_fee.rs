use crate::{
    are_assets_balanced, calculate_main_token_balance, InputOutputSetup, InputSelectionAlgorithm,
    InputSelectionResult, TransactionFeeEstimator,
};
use anyhow::anyhow;
use dcspark_core::tx::{UTxOBuilder, UTxODetails};
use dcspark_core::Balance;

#[derive(Default)]
pub struct FeeChangeBalancer {
    available_inputs: Vec<UTxODetails>,
}

impl InputSelectionAlgorithm for FeeChangeBalancer {
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
        if !are_assets_balanced(
            &input_output_setup.input_asset_balance,
            &input_output_setup.output_asset_balance,
        ) {
            return Err(anyhow!(
                "can't balance change when tokens are unbalanced. use other strategy"
            ));
        }

        let mut fee = estimator.min_required_fee()?;
        let current_balance = calculate_main_token_balance(
            &input_output_setup.input_balance,
            &input_output_setup.output_balance,
            &fee,
        );
        match current_balance {
            Balance::Debt(_d) => {
                return Err(anyhow!("there's not enough main token to balance change"));
            }
            Balance::Balanced => {
                // don't do anything
            }
            Balance::Excess(e) => {
                fee += &e;
            }
        }

        Ok(InputSelectionResult {
            input_balance: input_output_setup.input_balance,
            input_asset_balance: input_output_setup.input_asset_balance,
            output_balance: input_output_setup.output_balance,
            output_asset_balance: input_output_setup.output_asset_balance,
            fixed_inputs: input_output_setup.fixed_inputs,
            fixed_outputs: input_output_setup.fixed_outputs,
            chosen_inputs: vec![],
            changes: vec![],
            fee,
        })
    }

    fn available_inputs(&self) -> Vec<Self::InputUtxo> {
        self.available_inputs.clone()
    }
}
