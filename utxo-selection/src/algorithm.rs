use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::FeeEstimator;
use cardano_multiplatform_lib::address::Address;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;

pub trait InputSelectionAlgorithm<Estimate: FeeEstimator> {
    fn add_available_input(&mut self, input: InputBuilderResult) -> Result<(), JsError>;

    fn select_inputs(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup,
    ) -> Result<InputSelectionResult, JsError>;

    fn can_balance_change(&self) -> bool;
    fn balance_change(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup,
    ) -> Result<InputSelectionResult, JsError>;
}
