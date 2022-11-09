use crate::{
    common, InputOutputSetup, InputSelectionAlgorithm, TransactionFeeEstimator, UTxOBuilder,
};
use dcspark_core::tx::TransactionAsset;
use dcspark_core::{Address, Regulated, TokenId, Value};
use std::collections::HashMap;

#[allow(unused)]
pub enum PaymentEvent<InputUtxo: Clone, OutputUtxo: Clone> {
    Receiving { utxos: Vec<InputUtxo> },
    Paying { utxos: Vec<OutputUtxo> },
}

#[allow(unused)]
pub fn run_algorithm_benchmark<
    InputUtxo: Clone,
    OutputUtxo: Into<common::UTxOBuilder> + Clone,
    Estimator: TransactionFeeEstimator<InputUtxo = InputUtxo, OutputUtxo = OutputUtxo>,
    Algo: InputSelectionAlgorithm<InputUtxo = InputUtxo, OutputUtxo = OutputUtxo>,
    ChangeBalanceAlgo: InputSelectionAlgorithm<InputUtxo = InputUtxo, OutputUtxo = OutputUtxo>,
    EstimatorCreator,
    OutputToInput,
>(
    mut algorithm: Algo,
    mut balance_change_algo: ChangeBalanceAlgo,
    create_estimator: EstimatorCreator,
    initial_utxos: Vec<InputUtxo>,
    events: Vec<PaymentEvent<InputUtxo, OutputUtxo>>,
    change_address: Address,
    output_to_input: OutputToInput,
) -> anyhow::Result<Vec<InputUtxo>>
where
    EstimatorCreator: Fn(Vec<OutputUtxo>) -> anyhow::Result<Estimator>,
    OutputToInput: Fn(OutputUtxo) -> anyhow::Result<InputUtxo>,
{
    let mut current_utxos = initial_utxos;
    for event in events.into_iter() {
        let (output_balance, output_asset_balance, mut estimate, fixed_outputs) = match event {
            PaymentEvent::Receiving { utxos } => {
                utxos.into_iter().for_each(|utxo| current_utxos.push(utxo));
                continue;
            }
            PaymentEvent::Paying { utxos } => {
                let mut input_balance = Value::<Regulated>::zero();
                let mut asset_balance = HashMap::<TokenId, TransactionAsset>::new();
                for utxo in utxos.iter() {
                    let details: UTxOBuilder = utxo.clone().into();
                    input_balance += details.value;
                    for asset in details.assets.iter() {
                        let balance = asset_balance.entry(asset.fingerprint.clone()).or_insert(
                            TransactionAsset {
                                policy_id: asset.policy_id.clone(),
                                asset_name: asset.asset_name.clone(),
                                fingerprint: asset.fingerprint.clone(),
                                quantity: Value::zero(),
                            },
                        );
                        balance.quantity += asset.quantity.clone();
                    }
                }
                (
                    input_balance,
                    asset_balance,
                    create_estimator(utxos.clone())?,
                    utxos,
                )
            }
        };
        algorithm.set_available_inputs(current_utxos.clone())?;
        let mut select_result = algorithm.select_inputs(
            &mut estimate,
            InputOutputSetup {
                input_balance: Default::default(),
                input_asset_balance: Default::default(),
                output_balance,
                output_asset_balance,
                fixed_inputs: vec![],
                fixed_outputs,
                change_address: Some(change_address.clone()),
            },
        )?;

        current_utxos = algorithm.available_inputs();
        let mut changes = select_result.changes.clone();

        if !select_result.is_balanced() {
            balance_change_algo.set_available_inputs(current_utxos.clone())?;

            let mut fixed_inputs = select_result.fixed_inputs;
            fixed_inputs.append(&mut select_result.chosen_inputs);

            let mut fixed_outputs = select_result.fixed_outputs;
            fixed_outputs.append(&mut changes.clone());

            let mut balance_change_result = balance_change_algo.select_inputs(
                &mut estimate,
                InputOutputSetup {
                    input_balance: select_result.input_balance,
                    input_asset_balance: select_result.input_asset_balance,
                    output_balance: select_result.output_balance,
                    output_asset_balance: select_result.output_asset_balance,
                    fixed_inputs,
                    fixed_outputs,
                    change_address: Some(change_address.clone()),
                },
            )?;

            assert!(balance_change_result.is_balanced());

            current_utxos = balance_change_algo.available_inputs();
            changes.append(&mut balance_change_result.changes)
        }

        for change in changes {
            current_utxos.push(output_to_input(change)?);
        }
    }

    Ok(current_utxos)
}
