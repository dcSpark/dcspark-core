#[derive(Debug, Clone)]
pub struct InputOutputSetup {
    pub input_balance: cardano_multiplatform_lib::ledger::common::value::Value,
    pub output_balance: cardano_multiplatform_lib::ledger::common::value::Value,

    pub explicit_inputs:
        Vec<cardano_multiplatform_lib::builders::input_builder::InputBuilderResult>,
    pub explicit_outputs: Vec<cardano_multiplatform_lib::TransactionOutput>,

    pub fee: cardano_multiplatform_lib::ledger::common::value::Coin,
}

#[derive(Debug, Clone)]
pub struct InputSelectionResult {
    pub chosen_inputs: Vec<cardano_multiplatform_lib::builders::input_builder::InputBuilderResult>,
    pub chosen_outputs: Vec<cardano_multiplatform_lib::TransactionOutput>,
    pub input_balance: cardano_multiplatform_lib::ledger::common::value::Value,
    pub output_balance: cardano_multiplatform_lib::ledger::common::value::Value,

    pub fee: cardano_multiplatform_lib::ledger::common::value::Coin,
}
