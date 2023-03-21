use crate::common::{InputOutputSetup, InputSelectionResult};
use crate::estimate::TransactionFeeEstimator;
use dcspark_core::UTxOStore;

pub trait InputSelectionAlgorithm {
    type InputUtxo: Clone;
    type OutputUtxo: Clone;

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>>;
}

pub trait UTxOStoreSupport {
    fn set_available_utxos(&mut self, utxos: UTxOStore) -> anyhow::Result<()>;
    fn get_available_utxos(&mut self) -> anyhow::Result<UTxOStore>;
}
