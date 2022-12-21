use std::path::PathBuf;

use crate::tx_event::{address_from_pair, pair_from_address, TxEvent, TxOutput};
use anyhow::Context;
use clap::Parser;
use dcspark_core::tx::{TransactionAsset, TransactionId, UTxODetails, UtxoPointer};
use dcspark_core::{Address, AssetName, Balance, OutputIndex, PolicyId, Regulated, TokenId, Value};
use itertools::Itertools;
use serde::Deserialize;
use std::cmp::min;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tracing_subscriber::prelude::*;
use utxo_selection::{
    InputOutputSetup, InputSelectionAlgorithm, TransactionFeeEstimator, UTxOBuilder,
};

/* we don't take txs:
 * - with byron inputs
 * - with more than one staking key in inputs
 * - with no staking key in inputs
 */
fn is_supported_for_selection(inputs: &Vec<TxOutput>) -> Option<u64> {
    if inputs.iter().any(|input| input.address.is_none())
        || inputs.iter().map(|input| input.address).unique().count() != 1
    {
        return None;
    }
    return inputs
        .first()
        .map(|input| input.address.map(|(payment, stake)| stake).flatten())
        .flatten();
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
    input_events: PathBuf,
    output_insolvent: PathBuf,
    output_discarded: PathBuf,
    output_balance: PathBuf,
    output_balance_short: PathBuf,
    allow_balance_change: bool,
) -> anyhow::Result<()>
where
    EstimatorCreator: Fn() -> anyhow::Result<Estimator>,
{
    let mut staking_key_balance_computed =
        HashMap::<u64, HashMap<TokenId, Balance<Regulated>>>::new();
    let mut staking_key_balance_actual =
        HashMap::<u64, HashMap<TokenId, Balance<Regulated>>>::new();

    // staking key -> payment key -> utxos
    let mut address_computed_utxos_by_stake_key =
        HashMap::<u64, HashMap<u64, Vec<UTxODetails>>>::new();

    let mut insolvent_staking_keys = HashSet::<u64>::new();
    let mut discarded_staking_keys = HashSet::<u64>::new();

    let input_events = File::open(input_events)?;
    let input_events = BufReader::new(input_events);

    for (tx_number, event_str) in input_events.lines().enumerate() {
        let event = event_str?;
        let event: TxEvent = serde_json::from_str(&event)?;
        match event {
            TxEvent::Full { to, fee, from } => {
                let stake_key = is_supported_for_selection(&from);
                if stake_key.is_none() {
                    let stake_keys_to_discard: Vec<_> = from
                        .iter()
                        .map(|input| input.address.map(|addr| addr.1).flatten())
                        .filter(|input| input.is_some())
                        .map(|input| input.unwrap())
                        .collect();
                    for key in stake_keys_to_discard.into_iter() {
                        discarded_staking_keys.insert(key);
                        address_computed_utxos_by_stake_key.remove(&key);
                        staking_key_balance_computed.remove(&key);
                        staking_key_balance_actual.remove(&key);
                    }

                    // add balances
                    handle_partial_parsed(
                        tx_number,
                        to,
                        &mut address_computed_utxos_by_stake_key,
                        &mut staking_key_balance_computed,
                        &mut staking_key_balance_actual,
                        &insolvent_staking_keys,
                        &discarded_staking_keys,
                    );

                    continue;
                }
                let stake_key = stake_key.unwrap();
                if discarded_staking_keys.contains(&stake_key)
                    || insolvent_staking_keys.contains(&stake_key)
                {
                    // add balances
                    handle_partial_parsed(
                        tx_number,
                        to,
                        &mut address_computed_utxos_by_stake_key,
                        &mut staking_key_balance_computed,
                        &mut staking_key_balance_actual,
                        &insolvent_staking_keys,
                        &discarded_staking_keys,
                    );

                    continue;
                }
                // now we have inputs related to only one staking key. we're not insolvent and not discarded

                let change_addresses = get_change_addresses(stake_key, &to);
                let change_address_to_use =
                    choose_change_address(stake_key, &from, &change_addresses);

                let change_address_to_use = address_from_pair(change_address_to_use);

                let mut fixed_outputs: Vec<_> = get_non_change_outputs(&to, &change_addresses);
                if fixed_outputs.is_empty() {
                    fixed_outputs = outputs_to_builders(to.clone());
                }

                let mut total_output_balance = dcspark_core::Value::zero();
                let mut total_output_tokens = HashMap::<TokenId, TransactionAsset>::new();

                let mut estimate = create_estimator()?;

                for output in fixed_outputs.iter() {
                    estimate.add_output(output.clone())?;
                    total_output_balance += &output.value;
                    for asset in output.assets.iter() {
                        match total_output_tokens.entry(asset.fingerprint.clone()) {
                            Entry::Occupied(mut entry) => {
                                entry.get_mut().quantity += &asset.quantity;
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(asset.clone());
                            }
                        }
                    }
                }

                let mut available_utxos: Vec<UTxODetails> = vec![];
                let computed_utxos = address_computed_utxos_by_stake_key.get(&stake_key);
                let computed_utxos = match computed_utxos {
                    None => {
                        insolvent_staking_keys.insert(stake_key);
                        tracing::debug!(
                            "tx_number: {:?}, insolvent staking keys: {}",
                            tx_number,
                            insolvent_staking_keys.len()
                        );

                        // add balances
                        handle_partial_parsed(
                            tx_number,
                            to,
                            &mut address_computed_utxos_by_stake_key,
                            &mut staking_key_balance_computed,
                            &mut staking_key_balance_actual,
                            &insolvent_staking_keys,
                            &discarded_staking_keys,
                        );

                        continue;
                    }
                    Some(utxos) => utxos
                        .iter()
                        .flat_map(|(payment_key, utxos)| utxos.clone())
                        .collect::<Vec<_>>(),
                };

                algorithm.set_available_inputs(computed_utxos)?;
                let mut select_result = algorithm.select_inputs(
                    &mut estimate,
                    InputOutputSetup {
                        input_balance: Default::default(),
                        input_asset_balance: Default::default(),
                        output_balance: total_output_balance,
                        output_asset_balance: total_output_tokens,
                        fixed_inputs: vec![],
                        fixed_outputs: fixed_outputs.clone(),
                        change_address: Some(change_address_to_use.clone()),
                    },
                );

                let mut select_result = match select_result {
                    Ok(r) => r,
                    Err(err) => {
                        tracing::debug!(
                            "Can't select inputs for that address using provided algo {:?}, tx_number: {:?}",
                            err, tx_number,
                        );
                        insolvent_staking_keys.insert(stake_key);

                        // add balances
                        handle_partial_parsed(
                            tx_number,
                            to,
                            &mut address_computed_utxos_by_stake_key,
                            &mut staking_key_balance_computed,
                            &mut staking_key_balance_actual,
                            &insolvent_staking_keys,
                            &discarded_staking_keys,
                        );

                        continue;
                    }
                };

                let mut computed_available_utxos = algorithm.available_inputs();
                let mut changes = select_result.changes.clone();

                if !select_result.is_balanced() && allow_balance_change {
                    balance_change_algo.set_available_inputs(computed_available_utxos.clone())?;

                    // now all selected inputs are chosen ones
                    let mut fixed_inputs = select_result.fixed_inputs.clone();
                    fixed_inputs.append(&mut select_result.chosen_inputs.clone());

                    // outputs as well
                    let mut fixed_outputs = select_result.fixed_outputs.clone();
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
                            change_address: Some(change_address_to_use.clone()),
                        },
                    );

                    let mut balance_change_result = match balance_change_result {
                        Ok(r) => r,
                        Err(err) => {
                            tracing::debug!(
                                "Can't balance inputs for that address using provided algo {:?}, tx_number: {:?}",
                                err, tx_number
                            );
                            insolvent_staking_keys.insert(stake_key);

                            // add balances
                            handle_partial_parsed(
                                tx_number,
                                to,
                                &mut address_computed_utxos_by_stake_key,
                                &mut staking_key_balance_computed,
                                &mut staking_key_balance_actual,
                                &insolvent_staking_keys,
                                &discarded_staking_keys,
                            );

                            continue;
                        }
                    };

                    if !balance_change_result.is_balanced() {
                        tracing::debug!("Can't balance inputs for that address using provided algo event after running balance, tx_number: {:?}", tx_number);
                        insolvent_staking_keys.insert(stake_key);

                        // add balances
                        handle_partial_parsed(
                            tx_number,
                            to,
                            &mut address_computed_utxos_by_stake_key,
                            &mut staking_key_balance_computed,
                            &mut staking_key_balance_actual,
                            &insolvent_staking_keys,
                            &discarded_staking_keys,
                        );

                        continue;
                    }

                    computed_available_utxos = balance_change_algo.available_inputs();
                    // changes from first stage + changes from balance + original fixed outputs = all outputs
                    changes.append(&mut balance_change_result.changes)
                } else if !select_result.is_balanced() {
                    tracing::debug!("Can't balance inputs for that address using provided algo, tx_number: {:?}", tx_number);
                    insolvent_staking_keys.insert(stake_key);

                    // add balances
                    handle_partial_parsed(
                        tx_number,
                        to,
                        &mut address_computed_utxos_by_stake_key,
                        &mut staking_key_balance_computed,
                        &mut staking_key_balance_actual,
                        &insolvent_staking_keys,
                        &discarded_staking_keys,
                    );

                    continue;
                }

                let mut inputs_value = dcspark_core::Value::<Regulated>::zero();
                for change in select_result.changes.iter() {
                    inputs_value += &change.value;
                }
                for change in select_result.fixed_outputs.iter() {
                    inputs_value += &change.value;
                }
                inputs_value += &select_result.fee;
                for change in select_result.fixed_inputs.iter() {
                    inputs_value -= &change.value;
                }
                for change in select_result.chosen_inputs.iter() {
                    inputs_value -= &change.value;
                }
                assert_eq!(inputs_value, Value::zero());

                recount_available_inputs(
                    computed_available_utxos,
                    stake_key,
                    &mut address_computed_utxos_by_stake_key,
                    &mut staking_key_balance_computed,
                );

                let outputs: Vec<_> = fixed_outputs
                    .into_iter()
                    .chain(changes.into_iter())
                    .collect();
                add_new_selected_outputs_to_stake_keys(
                    tx_number,
                    outputs,
                    &mut address_computed_utxos_by_stake_key,
                    &mut staking_key_balance_computed,
                    &insolvent_staking_keys,
                    &discarded_staking_keys,
                );

                add_to_actual_balance(
                    &to,
                    &mut staking_key_balance_actual,
                    &insolvent_staking_keys,
                    &discarded_staking_keys,
                );
                subtract_from_actual_balance(stake_key, &from, &mut staking_key_balance_actual);
            }
            TxEvent::Partial { to } => {
                handle_partial_parsed(
                    tx_number,
                    to,
                    &mut address_computed_utxos_by_stake_key,
                    &mut staking_key_balance_computed,
                    &mut staking_key_balance_actual,
                    &insolvent_staking_keys,
                    &discarded_staking_keys,
                );
            }
        }

        if tx_number % 10000 == 0 {
            tracing::info!("Processed line {:?}", tx_number);
        }
    }

    for addr in insolvent_staking_keys.iter() {
        staking_key_balance_computed.remove(addr);
        staking_key_balance_actual.remove(addr);
        address_computed_utxos_by_stake_key.remove(addr);
    }

    tracing::info!(
        "Total converged addresses: {:?}",
        staking_key_balance_computed.len()
    );
    tracing::info!(
        "Total insolvent addresses: {:?}",
        insolvent_staking_keys.len()
    );
    tracing::info!(
        "Total discarded addresses: {:?}",
        discarded_staking_keys.len()
    );

    print_hashmap(discarded_staking_keys, output_discarded)?;
    print_hashmap(insolvent_staking_keys, output_insolvent)?;
    print_computed_balance(
        staking_key_balance_computed,
        staking_key_balance_actual,
        output_balance,
        output_balance_short,
    )?;

    Ok(())
}

fn print_hashmap(keys: HashSet<u64>, path: PathBuf) -> anyhow::Result<()> {
    let mut file = File::create(path)?;
    for key in keys.iter() {
        file.write_all(format!("{:?}\n", key).as_bytes())?;
    }
    Ok(())
}

fn print_computed_balance(
    staking_key_balance_computed: HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
    staking_key_balance_actual: HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
    output_balance: PathBuf,
    output_balance_short: PathBuf,
) -> anyhow::Result<()> {
    let mut output_balance = File::create(output_balance)?;
    let mut output_balance_short = File::create(output_balance_short)?;
    let keys = staking_key_balance_computed.iter();

    let mut better_than_actual: u64 = 0;
    let mut same_as_actual: u64 = 0;
    let mut worse_than_actual: u64 = 0;

    let mut non_checkable: u64 = 0;

    let mut not_found_actual: u64 = 0;
    let mut not_found_token_actual: u64 = 0;

    for (key, computed) in keys {
        let actual = if let Some(balance) = staking_key_balance_actual.get(key) {
            balance
        } else {
            not_found_actual += 1;
            output_balance.write_all(format!("no actual data: address: {:?}\n", key).as_bytes())?;
            continue;
        };
        let mut better_than_actual_element_wise = vec![];

        for (token, computed_token_balance) in computed.iter() {
            let actual_token_balance = match actual.get(token) {
                None => {
                    not_found_token_actual += 1;
                    output_balance.write_all(
                        format!(
                            "no token actual data: address: {:?}, token: {:?}\n",
                            key, token
                        )
                        .as_bytes(),
                    )?;
                    continue;
                }
                Some(b) => b,
            };
            let diff = match actual_token_balance {
                Balance::Debt(value) => computed_token_balance + value,
                Balance::Balanced => {
                    computed_token_balance + &dcspark_core::Value::<Regulated>::zero()
                }
                Balance::Excess(value) => computed_token_balance - value,
            };
            let print_value = match diff {
                Balance::Debt(value) => {
                    better_than_actual_element_wise.push(1);
                    format!("{:?}", value)
                }
                Balance::Balanced => {
                    better_than_actual_element_wise.push(0);
                    format!("{:?}", dcspark_core::Value::<Regulated>::zero())
                }
                Balance::Excess(value) => {
                    better_than_actual_element_wise.push(-1);
                    format!("{:?}", value)
                }
            };
            output_balance.write_all(
                format!(
                    "diff: address: {:?}, token: {:?}, diff: {:?}, actual: {:?}, computed: {:?}\n",
                    key, token, print_value, actual_token_balance, computed_token_balance
                )
                .as_bytes(),
            )?;
        }
        if better_than_actual_element_wise.iter().all(|b| *b == 1) {
            better_than_actual += 1;
        } else if better_than_actual_element_wise.iter().all(|b| *b == -1) {
            worse_than_actual += 1;
        } else if better_than_actual_element_wise.iter().all(|b| *b == 0) {
            same_as_actual += 1;
        } else {
            non_checkable += 1;
        }
    }

    output_balance_short
        .write_all(format!("better than actual: {:?}\n", better_than_actual).as_bytes())?;
    output_balance_short.write_all(format!("same as actual: {:?}\n", same_as_actual).as_bytes())?;
    output_balance_short
        .write_all(format!("worse than actual: {:?}\n", worse_than_actual).as_bytes())?;
    output_balance_short.write_all(format!("can't compare: {:?}\n", non_checkable).as_bytes())?;
    output_balance_short
        .write_all(format!("not found actual: {:?}\n", not_found_actual).as_bytes())?;
    output_balance_short
        .write_all(format!("not found token actual: {:?}\n", not_found_token_actual).as_bytes())?;

    Ok(())
}

fn handle_partial_parsed(
    tx_number: usize,
    to: Vec<TxOutput>,
    address_computed_utxos_by_stake_key: &mut HashMap<u64, HashMap<u64, Vec<UTxODetails>>>,
    staking_key_balance_computed: &mut HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
    staking_key_balance_actual: &mut HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
    insolvent_staking_keys: &HashSet<u64>,
    discarded_staking_keys: &HashSet<u64>,
) {
    add_to_actual_balance(
        &to,
        staking_key_balance_actual,
        &insolvent_staking_keys,
        &discarded_staking_keys,
    );
    add_untouched_outputs_to_stake_keys(
        tx_number,
        to,
        address_computed_utxos_by_stake_key,
        staking_key_balance_computed,
        &insolvent_staking_keys,
        &discarded_staking_keys,
    );
}

fn add_to_actual_balance(
    to: &Vec<TxOutput>,
    staking_key_balance_actual: &mut HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
    insolvent_keys: &HashSet<u64>,
    discarded_keys: &HashSet<u64>,
) {
    for output in to.iter() {
        let (_, staking) = match output.address.clone() {
            Some(addr) => addr,
            None => continue,
        };
        let staking = match staking {
            None => continue,
            Some(staking) => staking,
        };

        if insolvent_keys.contains(&staking) || discarded_keys.contains(&staking) {
            continue;
        }

        let balance = staking_key_balance_actual
            .entry(staking.clone())
            .or_default();
        *balance.entry(TokenId::MAIN).or_default() += &output.value;
        for token in output.assets.iter() {
            let asset = TransactionAsset::from(token.clone());
            *balance.entry(asset.fingerprint.clone()).or_default() += &asset.quantity;
        }
    }
}

fn subtract_from_actual_balance(
    staking_key: u64,
    from: &Vec<TxOutput>,
    staking_key_balance_actual: &mut HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
) {
    let balance = staking_key_balance_actual.entry(staking_key).or_default();

    for from in from.iter() {
        *balance.entry(TokenId::MAIN).or_default() -= &from.value;
        for token in from.assets.iter() {
            let asset = TransactionAsset::from(token.clone());
            *balance.entry(asset.fingerprint.clone()).or_default() -= &asset.quantity;
        }
    }
}

fn add_new_selected_outputs_to_stake_keys(
    tx_number: usize,
    outputs: Vec<UTxOBuilder>,
    address_computed_utxos_by_stake_key: &mut HashMap<u64, HashMap<u64, Vec<UTxODetails>>>,
    staking_key_balance_computed: &mut HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
    insolvent_keys: &HashSet<u64>,
    discarded_keys: &HashSet<u64>,
) {
    for (output_index, output) in outputs.iter().enumerate() {
        let (payment, staking) = match pair_from_address(output.address.clone()) {
            None => continue,
            Some(address) => address,
        };
        let staking = match staking {
            None => continue,
            Some(staking) => staking,
        };

        if insolvent_keys.contains(&staking) || discarded_keys.contains(&staking) {
            continue;
        }

        let current_stake_key_utxos = address_computed_utxos_by_stake_key
            .entry(staking.clone())
            .or_default();
        current_stake_key_utxos
            .entry(payment)
            .or_default()
            .push(UTxODetails {
                pointer: UtxoPointer {
                    transaction_id: TransactionId::new(tx_number.to_string()),
                    output_index: OutputIndex::new(output_index as u64),
                },
                address: output.address.clone(),
                value: output.value.clone(),
                assets: output.assets.clone(),
                metadata: Arc::new(Default::default()),
            });
        let current_token_balance = staking_key_balance_computed
            .entry(staking.clone())
            .or_default();

        *current_token_balance.entry(TokenId::MAIN).or_default() += &output.value;
        for token in output.assets.iter() {
            *current_token_balance
                .entry(token.fingerprint.clone())
                .or_default() += &token.quantity;
        }
    }
}

fn add_untouched_outputs_to_stake_keys(
    tx_number: usize,
    outputs: Vec<TxOutput>,
    address_computed_utxos_by_stake_key: &mut HashMap<u64, HashMap<u64, Vec<UTxODetails>>>,
    staking_key_balance_computed: &mut HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
    insolvent_keys: &HashSet<u64>,
    discarded_keys: &HashSet<u64>,
) {
    for (output_index, output) in outputs.iter().enumerate() {
        let (payment, staking) = match output.address {
            None => continue,
            Some(address) => address,
        };

        let staking = match staking {
            None => continue,
            Some(staking) => staking,
        };

        if insolvent_keys.contains(&staking) || discarded_keys.contains(&staking) {
            continue;
        }
        let assets: Vec<TransactionAsset> = output
            .assets
            .iter()
            .map(|asset| TransactionAsset::from(asset.clone()))
            .collect();

        let current_stake_key_utxos = address_computed_utxos_by_stake_key
            .entry(staking.clone())
            .or_default();
        current_stake_key_utxos
            .entry(payment)
            .or_default()
            .push(UTxODetails {
                pointer: UtxoPointer {
                    transaction_id: TransactionId::new(tx_number.to_string()),
                    output_index: OutputIndex::new(output_index as u64),
                },
                address: address_from_pair((payment, Some(staking))),
                value: output.value.clone(),
                assets: assets.clone(),
                metadata: Arc::new(Default::default()),
            });
        let current_token_balance = staking_key_balance_computed
            .entry(staking.clone())
            .or_default();

        *current_token_balance.entry(TokenId::MAIN).or_default() += &output.value;
        for token in assets.into_iter() {
            *current_token_balance
                .entry(token.fingerprint.clone())
                .or_default() += &token.quantity;
        }
    }
}

fn recount_available_inputs(
    computed_available_utxos: Vec<UTxODetails>,
    stake_key: u64,
    address_computed_utxos_by_stake_key: &mut HashMap<u64, HashMap<u64, Vec<UTxODetails>>>,
    staking_key_balance_computed: &mut HashMap<u64, HashMap<TokenId, Balance<Regulated>>>,
) {
    let current_stake_key_utxos = address_computed_utxos_by_stake_key
        .entry(stake_key)
        .or_default();
    current_stake_key_utxos.clear();
    let current_token_balance = staking_key_balance_computed
        .entry(stake_key.clone())
        .or_default();
    current_token_balance.clear();

    for available_input in computed_available_utxos.into_iter() {
        let (payment, _) = pair_from_address(available_input.address.clone()).unwrap();

        *current_token_balance.entry(TokenId::MAIN).or_default() += &available_input.value;
        for token in available_input.assets.iter() {
            *current_token_balance
                .entry(token.fingerprint.clone())
                .or_default() += &token.quantity;
        }

        current_stake_key_utxos
            .entry(payment)
            .or_default()
            .push(available_input);
    }
}

fn get_change_addresses(stake_key: u64, outputs: &Vec<TxOutput>) -> Vec<(u64, Option<u64>)> {
    let change_addresses: Vec<_> = outputs
        .iter()
        .filter(|output| output.address.is_some())
        .map(|output| output.address.unwrap())
        .filter(|addr| addr.1.is_some() && addr.1.clone().unwrap() == stake_key)
        .collect();

    change_addresses
}

fn get_non_change_outputs(
    outputs: &Vec<TxOutput>,
    change_addresses: &Vec<(u64, Option<u64>)>,
) -> Vec<UTxOBuilder> {
    let non_changes: Vec<_> = outputs
        .iter()
        .filter(|output| {
            output.address.is_none() || !change_addresses.contains(&output.address.clone().unwrap())
        })
        .cloned()
        .collect();
    let fixed_outputs = outputs_to_builders(non_changes);
    fixed_outputs
}

fn outputs_to_builders(outputs: Vec<TxOutput>) -> Vec<UTxOBuilder> {
    outputs
        .into_iter()
        .map(|output| {
            UTxOBuilder::new(
                output
                    .address
                    .map(|address| address_from_pair(address))
                    .unwrap_or(Address::new("byron".to_string())),
                output.value.clone(),
                output
                    .assets
                    .iter()
                    .map(|asset| TransactionAsset::from(asset.clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>()
}

fn choose_change_address(
    stake_key: u64,
    from: &Vec<TxOutput>,
    change_addresses: &Vec<(u64, Option<u64>)>,
) -> (u64, Option<u64>) {
    let first_from_with_stake_key = from
        .iter()
        // we always must find it
        .find(|from| from.address.is_some() && from.address.clone().unwrap().1 == Some(stake_key))
        .unwrap()
        .address
        .unwrap();
    change_addresses
        .first()
        .cloned()
        .unwrap_or(first_from_with_stake_key)
}