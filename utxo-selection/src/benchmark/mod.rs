use crate::{FeeEstimator, InputOutputSetup, InputSelectionAlgorithm, InputSelectionResult};
use cardano_multiplatform_lib::address::Address;
use cardano_multiplatform_lib::builders::input_builder::InputBuilderResult;
use cardano_multiplatform_lib::builders::tx_builder::{
    TransactionBuilder, TransactionBuilderConfig,
};
use cardano_multiplatform_lib::error::JsError;
use cardano_multiplatform_lib::ledger::common::value::Coin;
use cardano_multiplatform_lib::{Transaction, TransactionOutput};

pub struct TestSetup {
    available_utxos: Vec<InputBuilderResult>,
    transactions: Vec<Transaction>,
    change_address: Address,
}

pub struct TestSetupResult {
    available_utxos: Vec<TransactionOutput>,
}

pub trait FeeEstimatorForTest: FeeEstimator + Sized {
    fn from_tx(tx: &Transaction) -> Result<Self, JsError>;
}

pub fn tx_to_setup(_tx: Transaction) -> Result<InputOutputSetup, JsError> {
    todo!()
}

pub fn run_algorithm_benchmark<
    E: FeeEstimatorForTest,
    InputSelectionAlgo: InputSelectionAlgorithm<E>,
    BalanceExcessAlgo: InputSelectionAlgorithm<E>,
>(
    create_estimator: fn (Transaction) -> E,
    mut input_selection: InputSelectionAlgo,
    excess_balance_algo: BalanceExcessAlgo,
    setup: TestSetup,
) -> Result<(), JsError> {
    for utxo in setup.available_utxos.into_iter() {
        input_selection.add_available_input(utxo)?;
    }

    for tx in setup.transactions.into_iter() {
        let mut estimator = create_estimator(tx.clone());
        let selected_inputs = input_selection.select_inputs(&mut estimator, tx_to_setup(tx)?)?;

        todo!();
    }

    Ok(())
}

