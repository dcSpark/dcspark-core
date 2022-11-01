use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::error::JsError;
use cardano_multiplatform_lib::ledger::common::value::Coin;

///
/// This trait is designed to hide the fee calculation under abstraction.
/// The end-user of the library can choose themselves how to estimate the fees.
///
pub trait FeeEstimator {
    fn fee_for_input(&self, input: &InputBuilderResult) -> Result<Coin, JsError>;
    fn add_input(&self, input: &InputBuilderResult) -> Result<(), JsError>;

    fn fee_for_output(&self, input: &InputBuilderResult) -> Result<Coin, JsError>;
    fn add_output(&self, input: InputBuilderResult) -> Result<(), JsError>;
}
