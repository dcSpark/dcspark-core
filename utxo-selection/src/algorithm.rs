use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::TransactionFeeEstimator;
use dcspark_core::UTxOStore;

pub trait InputSelectionAlgorithm {
    type InputUtxo: Clone;
    type OutputUtxo: Clone;

    fn set_available_inputs(
        &mut self,
        available_inputs: Vec<Self::InputUtxo>,
    ) -> anyhow::Result<()>;

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>>;

    fn available_inputs(&self) -> Vec<Self::InputUtxo>;
}

pub trait UTxOStoreSupport {
    fn set_available_utxos(&mut self, utxos: UTxOStore) -> anyhow::Result<()>;
    fn get_available_utxos(&mut self) -> anyhow::Result<UTxOStore>;
}
