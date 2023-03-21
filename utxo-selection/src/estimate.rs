use dcspark_core::{Regulated, Value};

///
/// This trait is designed to hide the fee calculation under abstraction.
/// The end-user of the library can choose themselves how to estimate the fees.
///
pub trait TransactionFeeEstimator {
    type InputUtxo: Clone;
    type OutputUtxo: Clone;

    fn min_required_fee(&self) -> anyhow::Result<Value<Regulated>>;

    fn fee_for_input(&self, input: &Self::InputUtxo) -> anyhow::Result<Value<Regulated>>;
    fn add_input(&mut self, input: Self::InputUtxo) -> anyhow::Result<()>;

    fn fee_for_output(&self, output: &Self::OutputUtxo) -> anyhow::Result<Value<Regulated>>;
    fn add_output(&mut self, output: Self::OutputUtxo) -> anyhow::Result<()>;

    fn current_size(&self) -> anyhow::Result<usize>;
    fn max_size(&self) -> anyhow::Result<usize>;
}
