use dcspark_core::tx::TransactionAsset;
use dcspark_core::{Address, Balance, Regulated, TokenId, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct InputOutputSetup<InputUtxo: Clone, OutputUtxo: Clone> {
    pub input_balance: Value<Regulated>,
    pub input_asset_balance: HashMap<TokenId, TransactionAsset>,

    pub output_balance: Value<Regulated>,
    pub output_asset_balance: HashMap<TokenId, TransactionAsset>,

    pub fixed_inputs: Vec<InputUtxo>,
    pub fixed_outputs: Vec<OutputUtxo>,

    pub change_address: Option<Address>,
}

impl<InputUtxo: Clone, OutputUtxo: Clone> Default for InputOutputSetup<InputUtxo, OutputUtxo> {
    fn default() -> Self {
        Self {
            input_balance: Default::default(),
            input_asset_balance: Default::default(),
            output_balance: Default::default(),
            output_asset_balance: Default::default(),
            fixed_inputs: vec![],
            fixed_outputs: vec![],
            change_address: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputSelectionResult<InputUtxo: Clone, OutputUtxo: Clone> {
    pub input_balance: Value<Regulated>,
    pub input_asset_balance: HashMap<TokenId, TransactionAsset>,

    pub output_balance: Value<Regulated>,
    pub output_asset_balance: HashMap<TokenId, TransactionAsset>,

    pub balance: Balance<Regulated>,
    pub asset_balance: HashMap<TokenId, Balance<Regulated>>,

    pub fixed_inputs: Vec<InputUtxo>,
    pub fixed_outputs: Vec<OutputUtxo>,

    pub chosen_inputs: Vec<InputUtxo>,
    pub changes: Vec<OutputUtxo>,

    pub fee: Value<Regulated>,
}

impl<InputUtxo: Clone, OutputUtxo: Clone> InputSelectionResult<InputUtxo, OutputUtxo> {
    pub fn is_balanced(&self) -> bool {
        (self.balance.clone() - self.fee.clone()).balanced()
            && self
                .asset_balance
                .iter()
                .all(|(_, balance)| balance.balanced())
    }
}

#[derive(Clone)]
pub struct UTxOBuilder {
    pub address: Address,
    pub value: Value<Regulated>,
    pub assets: Vec<TransactionAsset>,
}

impl UTxOBuilder {
    pub fn new(address: Address, value: Value<Regulated>, assets: Vec<TransactionAsset>) -> Self {
        Self {
            address,
            value,
            assets,
        }
    }
}
