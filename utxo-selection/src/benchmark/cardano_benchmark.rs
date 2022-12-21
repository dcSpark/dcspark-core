use std::path::PathBuf;

use crate::{
    DummyCmlFeeEstimate, InputOutputSetup, InputSelectionAlgorithm, TransactionFeeEstimator,
    UTxOBuilder,
};
use anyhow::{anyhow, Context};
use cardano_multiplatform_lib::crypto::TransactionHash;
use cardano_multiplatform_lib::ledger::common::value::{BigNum, Coin};
use cardano_multiplatform_lib::{TransactionInput, TransactionOutput};
use cardano_utils::conversion::csl_value_to_tokens;
use dcspark_core::tx::{TransactionAsset, TransactionId, UTxODetails, UtxoPointer};
use dcspark_core::{Balance, OutputIndex, Regulated, TokenId, UTxOStore, Value};
use deps::serde_json;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::{max, min};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TxOutputIntent {
    address: Option<cardano_multiplatform_lib::address::Address>,
    amount: cardano_multiplatform_lib::ledger::common::value::Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum TxEvent {
    // every from is shelley address. `to` can be any address
    // we store all amounts in this case: either shelley or byron outputs amounts, since
    // we can perform the selection for that
    FromParsed {
        to: Vec<TxOutputIntent>,
        fee: cardano_multiplatform_lib::ledger::common::value::Coin,
        // we can assume we can spend utxos with both credentials if we have multiple froms
        from: Vec<TxOutputIntent>,
    },
    // this applies when some of the from addresses are byron
    // we store shelley from and to intents
    // this way we can compute the balance change afterwards
    PartialParsed {
        to: Vec<TxOutputIntent>,
        // we store how much money we spent from the parsed addresses
        from: Vec<TxOutputIntent>,
    },
    Unparsed {
        tx: TransactionHash,
    },
}

fn handle_partial_parsed<
    Algo: InputSelectionAlgorithm<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>,
>(
    algorithm: &mut Algo,
    to: Vec<TxOutputIntent>,
    from: Vec<TxOutputIntent>,
    address_blockchain_balance: &mut HashMap<
        dcspark_core::Address,
        HashMap<TokenId, Balance<Regulated>>,
    >,
    address_computed_balance: &mut HashMap<
        dcspark_core::Address,
        HashMap<TokenId, Balance<Regulated>>,
    >,
    address_computed_utxos: &mut HashMap<dcspark_core::Address, Vec<UTxODetails>>,
    number: usize,
) -> anyhow::Result<()> {
    let mut address_to_total_balance_used =
        HashMap::<dcspark_core::Address, HashMap<TokenId, Value<Regulated>>>::new();
    let mut token_id_to_asset = HashMap::<TokenId, TransactionAsset>::new();
    for from_utxo in from.iter() {
        let address = dcspark_core::Address::new(
            from_utxo
                .address
                .as_ref()
                .map(|addr| addr.to_bech32(None).unwrap())
                .unwrap_or("N/A".to_string()),
        );
        let (balance, tokens) = csl_value_to_tokens(&from_utxo.amount)?;

        let entry = address_to_total_balance_used
            .entry(address.clone())
            .or_default();
        *entry.entry(TokenId::MAIN).or_default() += balance.clone();
        for (token, asset) in tokens.iter() {
            *entry.entry(token.clone()).or_default() += asset.quantity.clone();
            token_id_to_asset.insert(
                token.clone(),
                TransactionAsset {
                    policy_id: asset.policy_id.clone(),
                    asset_name: asset.asset_name.clone(),
                    fingerprint: asset.fingerprint.clone(),
                    quantity: Default::default(),
                },
            );
        }

        let abb = address_blockchain_balance.entry(address).or_default();
        *abb.entry(TokenId::MAIN).or_default() -= balance;
        for (token, asset) in tokens.iter() {
            *abb.entry(token.clone()).or_default() -= asset.quantity.clone();
        }
    }

    for (address, balance) in address_to_total_balance_used {
        let address_utxos = address_computed_utxos
            .get(&address)
            .cloned()
            .unwrap_or_default();
        algorithm.set_available_inputs(address_utxos.clone())?;
        let mut output_asset_balance = HashMap::new();
        for (token, balance) in balance.iter() {
            if token != &TokenId::MAIN {
                output_asset_balance
                    .entry(token.clone())
                    .or_insert(token_id_to_asset.get(&token.clone()).cloned().unwrap())
                    .quantity += balance.clone();
            }
        }
        let mut select_result = algorithm.select_inputs(
            &mut DummyCmlFeeEstimate::<UTxODetails, UTxOBuilder>::new(),
            InputOutputSetup {
                input_balance: Default::default(),
                input_asset_balance: Default::default(),
                output_balance: balance.get(&TokenId::MAIN).cloned().unwrap_or_default(),
                output_asset_balance: output_asset_balance.clone(),
                fixed_inputs: vec![],
                fixed_outputs: vec![UTxOBuilder {
                    address: dcspark_core::Address::new("N/A"),
                    value: balance.get(&TokenId::MAIN).cloned().unwrap_or_default(),
                    assets: output_asset_balance
                        .iter()
                        .map(|(token, asset)| asset.clone())
                        .collect(),
                }],
                change_address: Some(dcspark_core::Address::new("N/A")),
            },
        )?;

        let address_utxos = algorithm.available_inputs();
        let mut changes = select_result.changes.clone();

        // recalculate balance and change utxos
        address_computed_utxos.insert(address.clone(), address_utxos.clone());
        let balance = address_computed_balance.entry(address.clone()).or_default();
        balance.clear();
        for utxo in address_utxos.iter() {
            *balance.entry(TokenId::MAIN).or_default() += utxo.value.clone();
            for token in utxo.assets.iter() {
                *balance.entry(token.fingerprint.clone()).or_default() += token.quantity.clone();
            }
        }
    }

    for (output_index, to_utxo) in to.iter().enumerate() {
        let address = dcspark_core::Address::new(
            to_utxo
                .address
                .as_ref()
                .map(|addr| addr.to_bech32(None).unwrap())
                .unwrap_or("N/A".to_string()),
        );
        let (balance, tokens) = csl_value_to_tokens(&to_utxo.amount)?;

        let computed_utxos = address_computed_utxos.entry(address.clone()).or_default();
        computed_utxos.push(UTxODetails {
            pointer: UtxoPointer {
                transaction_id: TransactionId::new(number.to_string()),
                output_index: OutputIndex::new(output_index as u64),
            },
            address: address.clone(),
            value: balance.clone(),
            assets: tokens.iter().map(|(token, asset)| asset.clone()).collect(),
            metadata: Arc::new(Default::default()),
        });

        let computed_balance = address_computed_balance.entry(address.clone()).or_default();
        *computed_balance.entry(TokenId::MAIN).or_default() += balance.clone();
        for (token, asset) in tokens.iter() {
            *computed_balance.entry(token.clone()).or_default() += asset.quantity.clone();
        }

        let abb = address_blockchain_balance
            .entry(address.clone())
            .or_default();
        *abb.entry(TokenId::MAIN).or_default() += balance;
        for (token, asset) in tokens.iter() {
            *abb.entry(token.clone()).or_default() += asset.quantity.clone();
        }
    }
    Ok(())
}

#[allow(unused)]
pub fn run_algorithm_benchmark<
    Estimator: TransactionFeeEstimator<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>,
    Algo: InputSelectionAlgorithm<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>,
    ChangeBalanceAlgo: InputSelectionAlgorithm<InputUtxo = UTxODetails, OutputUtxo = UTxOBuilder>,
    EstimatorCreator,
>(
    mut algorithm: Algo,
    mut balance_change_algo: ChangeBalanceAlgo,
    create_estimator: EstimatorCreator,
    file: PathBuf,
    out: PathBuf,
) -> anyhow::Result<()>
where
    EstimatorCreator: Fn() -> anyhow::Result<Estimator>,
{
    let mut address_blockchain_balance =
        HashMap::<dcspark_core::Address, HashMap<TokenId, Balance<Regulated>>>::new();
    let mut address_computed_balance =
        HashMap::<dcspark_core::Address, HashMap<TokenId, Balance<Regulated>>>::new();

    let mut address_computed_utxos = HashMap::<dcspark_core::Address, Vec<UTxODetails>>::new();

    let file = File::open(file)?;
    let reader = BufReader::new(file);

    for (number, line) in reader.lines().enumerate() {
        let line = line?;
        let event: TxEvent = serde_json::from_str(&line)?;
        match event {
            TxEvent::FromParsed { to, fee, from } => {
                //                 let addresses: Vec<dcspark_core::Address> = from.iter().map(|cred| dcspark_core::Address::new(hex::encode(cred.to_bytes()))).collect();
                let mut addresses: Vec<dcspark_core::Address> = vec![];

                for from_utxo in from.iter() {
                    if let Some(address) = &from_utxo.address {
                        let address = dcspark_core::Address::new(
                            from_utxo
                                .address
                                .as_ref()
                                .map(|addr| addr.to_bech32(None).unwrap())
                                .unwrap_or("N/A".to_string()),
                        );
                        addresses.push(address.clone());
                    }
                }

                let utxo_stores: Vec<Option<Vec<UTxODetails>>> = addresses
                    .iter()
                    .map(|address| address_computed_utxos.get(address).cloned())
                    .collect();
                let mut current_utxos: Vec<UTxODetails> = vec![];
                for store in utxo_stores {
                    if let Some(store) = store {
                        current_utxos.append(&mut store.clone());
                    }
                }

                let addresses: HashSet<dcspark_core::Address> =
                    HashSet::from_iter(addresses.into_iter());

                // compute output balance and output asset balance
                let mut total_output_balance = Value::zero();
                let mut total_output_tokens = HashMap::<TokenId, TransactionAsset>::new();

                // get changes
                let mut original_changes = Vec::<TxOutputIntent>::new();

                // add fixed outputs as well
                let mut original_fixed_outputs = vec![];

                for to_utxo in to.iter() {
                    let mut is_change = false;
                    let address = if let Some(address) = &to_utxo.address {
                        let address = dcspark_core::Address::new(
                            address.to_bech32(None).unwrap_or("N/A".to_string()),
                        );
                        if addresses.contains(&address) {
                            // this is a change
                            is_change = true;
                        }
                        address
                    } else {
                        dcspark_core::Address::new("N/A")
                    };

                    let (output_balance, output_tokens) = csl_value_to_tokens(&to_utxo.amount)?;

                    // changes are not fixed outputs
                    if is_change {
                        original_changes.push(to_utxo.clone());
                        continue;
                    }
                    // fixed output is an intent to send money to someone else
                    let fixed_output = UTxOBuilder::new(
                        address,
                        output_balance.clone(),
                        output_tokens.values().into_iter().cloned().collect(),
                    );
                    original_fixed_outputs.push(fixed_output);

                    total_output_balance += output_balance;
                    for (token, asset) in output_tokens.into_iter() {
                        total_output_tokens
                            .entry(token)
                            .or_insert(TransactionAsset {
                                policy_id: asset.policy_id,
                                asset_name: asset.asset_name,
                                fingerprint: asset.fingerprint,
                                quantity: Default::default(),
                            })
                            .quantity += asset.quantity;
                    }
                }

                // if we didn't have changes we use the first address from input address
                let change_address = if original_changes.is_empty() {
                    addresses.iter().next().cloned()
                } else {
                    // if we had changes we use first change address
                    original_changes
                        .first()
                        .map(|change| {
                            change.address.clone().map(|address| {
                                dcspark_core::Address::new(hex::encode(address.to_bytes()))
                            })
                        })
                        .flatten()
                };

                let mut estimate = create_estimator()?;
                for output in original_fixed_outputs.iter() {
                    estimate.add_output(output.clone())?;
                }
                algorithm.set_available_inputs(current_utxos.clone())?;
                let mut select_result = algorithm.select_inputs(
                    &mut estimate,
                    InputOutputSetup {
                        input_balance: Default::default(),
                        input_asset_balance: Default::default(),
                        output_balance: total_output_balance,
                        output_asset_balance: total_output_tokens,
                        fixed_inputs: vec![],
                        fixed_outputs: original_fixed_outputs.clone(),
                        change_address: change_address.clone(),
                    },
                );

                if let Err(res) = select_result {
                    println!("Can't select inputs for that address using provided algo");
                    handle_partial_parsed(
                        &mut algorithm,
                        to,
                        from,
                        &mut address_blockchain_balance,
                        &mut address_computed_balance,
                        &mut address_computed_utxos,
                        number,
                    )?;
                    continue;
                }
                let mut select_result = select_result?;

                // once the selection is performed we might need to balance change

                current_utxos = algorithm.available_inputs();
                let mut changes = select_result.changes.clone();

                if !select_result.is_balanced() {
                    balance_change_algo.set_available_inputs(current_utxos.clone())?;

                    // now all selected inputs are chosen ones
                    let mut fixed_inputs = select_result.fixed_inputs;
                    fixed_inputs.append(&mut select_result.chosen_inputs);

                    // outputs as well
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
                            change_address: change_address.clone(),
                        },
                    );

                    if let Err(res) = balance_change_result {
                        println!("Can't balance change for that address using provided algo");
                        handle_partial_parsed(
                            &mut algorithm,
                            to,
                            from,
                            &mut address_blockchain_balance,
                            &mut address_computed_balance,
                            &mut address_computed_utxos,
                            number,
                        )?;
                        continue;
                    }

                    let mut balance_change_result = balance_change_result?;
                    assert!(balance_change_result.is_balanced());

                    current_utxos = balance_change_algo.available_inputs();
                    // changes from first stage + changes from balance + original fixed outputs = all outputs
                    changes.append(&mut balance_change_result.changes)
                }

                // now we replace available inputs for addresses with the ones that are left at that step
                for address in addresses.iter() {
                    *address_computed_utxos.entry(address.clone()).or_default() = current_utxos
                        .iter()
                        .filter(|utxo| utxo.address == address.clone())
                        .cloned()
                        .collect();
                    let balance = address_computed_balance.entry(address.clone()).or_default();
                    balance.clear();
                    for utxo in current_utxos.iter() {
                        *balance.entry(TokenId::MAIN).or_default() += utxo.value.clone();
                        for token in utxo.assets.iter() {
                            *balance.entry(token.fingerprint.clone()).or_default() +=
                                token.quantity.clone();
                        }
                    }
                }

                // we go through the outputs and add the outputs to the available inputs
                for (output_index, output) in original_fixed_outputs
                    .iter()
                    .chain(changes.iter())
                    .enumerate()
                {
                    let entry = address_computed_utxos
                        .entry(output.address.clone())
                        .or_default();
                    entry.push(UTxODetails {
                        pointer: UtxoPointer {
                            transaction_id: TransactionId::new(number.to_string()),
                            output_index: OutputIndex::new(output_index as u64),
                        },
                        address: output.address.clone(),
                        value: output.value.clone(),
                        assets: output.assets.clone(),
                        metadata: Arc::new(Default::default()),
                    });
                    let balance = address_computed_balance
                        .entry(output.address.clone())
                        .or_default();
                    *balance.entry(TokenId::MAIN).or_default() += output.value.clone();
                    for token in output.assets.iter() {
                        *balance.entry(token.fingerprint.clone()).or_default() +=
                            token.quantity.clone();
                    }
                }

                for from_utxo in from.iter() {
                    if let Some(address) = &from_utxo.address {
                        let address = dcspark_core::Address::new(
                            from_utxo
                                .address
                                .as_ref()
                                .map(|addr| addr.to_bech32(None).unwrap())
                                .unwrap_or("N/A".to_string()),
                        );

                        let (input, input_tokens) = csl_value_to_tokens(&from_utxo.amount)?;

                        // update original blockchain balance
                        let original_blockchain_balance = address_blockchain_balance
                            .entry(address.clone())
                            .or_default();
                        *original_blockchain_balance
                            .entry(TokenId::MAIN)
                            .or_default() -= input.clone();
                        for (token, asset) in input_tokens.iter() {
                            *original_blockchain_balance
                                .entry(token.clone())
                                .or_default() -= asset.quantity.clone();
                        }
                    }
                }

                for to_utxo in to.iter() {
                    let address = if let Some(address) = &to_utxo.address {
                        let address = dcspark_core::Address::new(
                            address.to_bech32(None).unwrap_or("N/A".to_string()),
                        );
                        address
                    } else {
                        dcspark_core::Address::new("N/A")
                    };

                    let (output_balance, output_tokens) = csl_value_to_tokens(&to_utxo.amount)?;

                    // update original blockchain balance
                    let original_blockchain_balance = address_blockchain_balance
                        .entry(address.clone())
                        .or_default();
                    *original_blockchain_balance
                        .entry(TokenId::MAIN)
                        .or_default() += output_balance.clone();
                    for (token, asset) in output_tokens.iter() {
                        *original_blockchain_balance
                            .entry(token.clone())
                            .or_default() += asset.quantity.clone();
                    }
                }
            }
            TxEvent::PartialParsed { to, from } => {
                handle_partial_parsed(
                    &mut algorithm,
                    to,
                    from,
                    &mut address_blockchain_balance,
                    &mut address_computed_balance,
                    &mut address_computed_utxos,
                    number,
                )?;
            }
            TxEvent::Unparsed { .. } => {}
        }
        if number % 10000 == 0 {
            println!("Processed line {:?}", number);
        }
    }

    let mut out = File::create(out)?;

    for (address, tokens) in address_blockchain_balance.iter() {
        out.write_all(format!("{:?}\nblockchain:\n", address).as_bytes());
        for (token, balance) in tokens.iter() {
            out.write_all(format!("{:?}: {:?}\n", token, balance).as_bytes());
        }
        out.write_all(format!("computed:\n").as_bytes());
        if let Some(entry) = address_computed_balance.get(address) {
            for (token, balance) in entry.iter() {
                out.write_all(format!("{:?}: {:?}\n", token, balance).as_bytes());
            }
        }
    }

    Ok(())
}
