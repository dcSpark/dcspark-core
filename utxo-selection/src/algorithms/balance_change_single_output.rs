use crate::{
    calculate_asset_balance, calculate_main_token_balance, InputOutputSetup,
    InputSelectionAlgorithm, InputSelectionResult, TransactionFeeEstimator,
};
use anyhow::anyhow;
use dcspark_core::tx::{TransactionAsset, UTxOBuilder, UTxODetails};
use dcspark_core::{Address, Balance, Regulated, Value};

pub struct SingleOutputChangeBalancer {
    address: Address,
    extra: Option<String>,
}

impl SingleOutputChangeBalancer {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            extra: None,
        }
    }

    pub fn set_extra(&mut self, extra: String) {
        self.extra = Some(extra);
    }
}

impl InputSelectionAlgorithm for SingleOutputChangeBalancer {
    type InputUtxo = UTxODetails;
    type OutputUtxo = UTxOBuilder;

    fn select_inputs<
        Estimate: TransactionFeeEstimator<InputUtxo = Self::InputUtxo, OutputUtxo = Self::OutputUtxo>,
    >(
        &mut self,
        estimator: &mut Estimate,
        input_output_setup: InputOutputSetup<Self::InputUtxo, Self::OutputUtxo>,
    ) -> anyhow::Result<InputSelectionResult<Self::InputUtxo, Self::OutputUtxo>> {
        let asset_balances = calculate_asset_balance(
            &input_output_setup.input_asset_balance,
            &input_output_setup.output_asset_balance,
        );
        let mut change_assets = vec![];
        for (token, asset_balance) in asset_balances.into_iter() {
            match asset_balance {
                Balance::Debt(d) => {
                    return Err(anyhow!(
                        "there's lack of assets selected, can't balance change: {}",
                        d
                    ));
                }
                Balance::Balanced => {}
                Balance::Excess(excess) => {
                    let mut asset = input_output_setup
                        .input_asset_balance
                        .get(&token)
                        .ok_or_else(|| anyhow!("asset {} must be presented in the inputs", token))?
                        .clone();
                    asset.quantity = excess;
                    change_assets.push(asset)
                }
            }
        }

        let mut fee = estimator.min_required_fee()?;
        let current_balance = calculate_main_token_balance(
            &input_output_setup.input_balance,
            &input_output_setup.output_balance,
            &fee,
        );

        let value: Value<Regulated> = match current_balance {
            Balance::Debt(d) => {
                return Err(anyhow!(
                    "there's lack of main asset selected, can't balance change: {}",
                    d
                ));
            }
            Balance::Balanced => Value::zero(),
            Balance::Excess(excess) => excess,
        };

        let mut change = UTxOBuilder {
            address: self.address.clone(),
            value,
            assets: change_assets,
            extra: self.extra.clone(),
        };

        let fee_for_change = estimator.fee_for_output(&change)?;
        change.value -= &fee_for_change;
        fee += &fee_for_change;

        let output_balance = &input_output_setup.output_balance + &change.value;
        let mut output_asset_balance = input_output_setup.output_asset_balance;
        for asset in change.assets.iter() {
            output_asset_balance
                .entry(asset.fingerprint.clone())
                .or_insert(TransactionAsset::new(
                    asset.policy_id.clone(),
                    asset.asset_name.clone(),
                    asset.fingerprint.clone(),
                ))
                .quantity += &asset.quantity;
        }
        Ok(InputSelectionResult {
            input_balance: input_output_setup.input_balance,
            input_asset_balance: input_output_setup.input_asset_balance,
            output_balance,
            output_asset_balance,
            fixed_inputs: input_output_setup.fixed_inputs,
            fixed_outputs: input_output_setup.fixed_outputs,
            chosen_inputs: vec![],
            changes: vec![change],
            fee,
        })
    }
}
