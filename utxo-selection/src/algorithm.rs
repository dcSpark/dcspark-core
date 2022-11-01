use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::FeeEstimator;
use cardano_multiplatform_lib::error::JsError;

pub trait InputSelectionAlgorithm<Estimate: FeeEstimator> {
    fn select_inputs(
        self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup,
    ) -> Result<InputSelectionResult, JsError>;
}
