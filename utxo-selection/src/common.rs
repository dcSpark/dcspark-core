use dcspark_core::tx::{TransactionAsset, UTxOBuilder, UTxODetails};
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

impl InputOutputSetup<UTxODetails, UTxOBuilder> {
    pub fn from_fixed_inputs_and_outputs(
        inputs: Vec<UTxODetails>,
        outputs: Vec<UTxOBuilder>,
        change_address: Option<Address>,
    ) -> Self {
        let mut input_balance = Value::<Regulated>::zero();
        let mut input_asset_balance = HashMap::<TokenId, TransactionAsset>::new();

        for input in inputs.iter() {
            input_balance += &input.value;
            for asset in input.assets.iter() {
                input_asset_balance
                    .entry(asset.fingerprint.clone())
                    .or_insert(TransactionAsset::new(
                        asset.policy_id.clone(),
                        asset.asset_name.clone(),
                        asset.fingerprint.clone(),
                    ))
                    .quantity += &asset.quantity;
            }
        }

        let mut output_balance = Value::<Regulated>::zero();
        let mut output_asset_balance = HashMap::<TokenId, TransactionAsset>::new();
        for output in outputs.iter() {
            output_balance += &output.value;
            for asset in output.assets.iter() {
                output_asset_balance
                    .entry(asset.fingerprint.clone())
                    .or_insert(TransactionAsset::new(
                        asset.policy_id.clone(),
                        asset.asset_name.clone(),
                        asset.fingerprint.clone(),
                    ))
                    .quantity += &asset.quantity;
            }
        }
        Self {
            input_balance,
            input_asset_balance,
            output_balance,
            output_asset_balance,
            fixed_inputs: inputs,
            fixed_outputs: outputs,
            change_address,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputSelectionResult<InputUtxo: Clone, OutputUtxo: Clone> {
    pub input_balance: Value<Regulated>,
    pub input_asset_balance: HashMap<TokenId, TransactionAsset>,

    pub output_balance: Value<Regulated>,
    pub output_asset_balance: HashMap<TokenId, TransactionAsset>,

    pub fixed_inputs: Vec<InputUtxo>,
    pub fixed_outputs: Vec<OutputUtxo>,

    pub chosen_inputs: Vec<InputUtxo>,
    pub changes: Vec<OutputUtxo>,

    pub fee: Value<Regulated>,
}

pub fn calculate_main_token_balance(
    input_balance: &Value<Regulated>,
    output_balance: &Value<Regulated>,
    fee: &Value<Regulated>,
) -> Balance<Regulated> {
    let mut balance = Balance::zero();
    balance += input_balance;
    balance -= fee;
    balance -= output_balance;
    balance
}

pub fn calculate_asset_balance(
    input_asset_balance: &HashMap<TokenId, TransactionAsset>,
    output_asset_balance: &HashMap<TokenId, TransactionAsset>,
) -> HashMap<TokenId, Balance<Regulated>> {
    let mut token_balances = HashMap::<TokenId, Balance<Regulated>>::new();
    for (token, asset) in input_asset_balance.iter() {
        *token_balances.entry(token.clone()).or_default() += &asset.quantity;
    }
    for (token, asset) in output_asset_balance.iter() {
        *token_balances.entry(token.clone()).or_default() -= &asset.quantity;
    }
    token_balances
}

pub fn are_assets_balanced(
    input_asset_balance: &HashMap<TokenId, TransactionAsset>,
    output_asset_balance: &HashMap<TokenId, TransactionAsset>,
) -> bool {
    let token_balances = calculate_asset_balance(input_asset_balance, output_asset_balance);
    for balance in token_balances.values() {
        if !balance.balanced() {
            return false;
        }
    }
    true
}

impl<InputUtxo: Clone, OutputUtxo: Clone> InputSelectionResult<InputUtxo, OutputUtxo> {
    pub fn is_balanced(&self) -> bool {
        let ada_balanced =
            calculate_main_token_balance(&self.input_balance, &self.output_balance, &self.fee);
        if !ada_balanced.balanced() {
            return false;
        }

        are_assets_balanced(&self.input_asset_balance, &self.output_asset_balance)
    }
}

impl InputSelectionResult<UTxODetails, UTxOBuilder> {
    pub fn are_utxos_balanced(&self) -> bool {
        if !self.is_balanced() {
            return false;
        }

        let mut tokens_map = HashMap::<TokenId, Balance<Regulated>>::new();
        for input in self.fixed_inputs.iter().chain(self.chosen_inputs.iter()) {
            *tokens_map.entry(TokenId::MAIN).or_default() += &input.value;
            for asset in input.assets.iter() {
                *tokens_map.entry(asset.fingerprint.clone()).or_default() += &asset.quantity;
            }
        }
        for output in self.fixed_outputs.iter().chain(self.changes.iter()) {
            *tokens_map.entry(TokenId::MAIN).or_default() -= &output.value;
            for asset in output.assets.iter() {
                *tokens_map.entry(asset.fingerprint.clone()).or_default() -= &asset.quantity;
            }
        }
        *tokens_map.entry(TokenId::MAIN).or_default() -= &self.fee;
        for balance in tokens_map.values() {
            if !balance.balanced() {
                return false;
            }
        }
        true
    }
}
