use std::path::PathBuf;

use anyhow::{anyhow, Context};
use cardano_multiplatform_lib::address::StakeCredential;
use cardano_multiplatform_lib::crypto::TransactionHash;
use cardano_multiplatform_lib::ledger::common::value::BigNum;
use cardano_multiplatform_lib::PolicyID;
use clap::Parser;
use dcspark_core::Regulated;
use entity::sea_orm::Database;
use entity::sea_orm::QueryFilter;
use entity::{
    prelude::*,
    sea_orm::{prelude::*, ColumnTrait, Condition, EntityTrait, QueryOrder, QuerySelect},
};
use serde::Deserialize;
use std::cmp::min;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use tracing_subscriber::prelude::*;
use utxo_selection_benchmark::mapper::DataMapper;
use utxo_selection_benchmark::tx_event::{TxAsset, TxEvent, TxOutput};
use utxo_selection_benchmark::utils::{dump_hashmap_to_file, dump_hashset_to_file};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum DbConfig {
    Postgres {
        host: String,
        port: u64,
        user: String,
        password: String,
        db: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    db: DbConfig,
    unparsed_transactions: PathBuf,
    payment_creds_mapping: PathBuf,
    staking_creds_mapping: PathBuf,
    policy_mapping: PathBuf,
    asset_name_mapping: PathBuf,
    address_to_mapping: PathBuf,
    banned_addresses: PathBuf,
    events_output_path: PathBuf,
    cleaned_events_output_path: PathBuf,
    tx_per_page: i64,
}

#[derive(Parser, Debug)]
#[clap(version)]
pub struct Cli {
    /// path to config file
    #[clap(long, value_parser)]
    config_path: PathBuf,
}

#[tokio::main]
async fn main() {
    let result = _main().await;
    result.unwrap();
}

async fn _main() -> anyhow::Result<()> {
    // Start logging setup block
    let fmt_layer = tracing_subscriber::fmt::layer().with_test_writer();

    let sqlx_filter = tracing_subscriber::filter::Targets::new()
        // sqlx logs every SQL query and how long it took which is very noisy
        .with_target("sqlx", tracing::Level::WARN)
        .with_default(tracing_subscriber::fmt::Subscriber::DEFAULT_MAX_LEVEL);

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(sqlx_filter)
        .init();

    let Cli { config_path } = Cli::parse();

    tracing::info!("Config file {:?}", config_path);
    let file = File::open(&config_path).with_context(|| {
        format!(
            "Cannot read config file {path}",
            path = config_path.display()
        )
    })?;
    let config: Config = serde_yaml::from_reader(file).with_context(|| {
        format!(
            "Cannot read config file {path}",
            path = config_path.display()
        )
    })?;
    let (user, password, host, port, db) = match config.db {
        DbConfig::Postgres {
            host,
            port,
            user,
            password,
            db,
        } => (user, password, host, port, db),
    };

    let url = format!("postgresql://{user}:{password}@{host}:{port}/{db}");
    tracing::info!("Connection url {:?}", url);
    let conn = Database::connect(&url).await?;
    tracing::info!("Connection success");

    /////////
    let shelley_first_blocks = Block::find()
        .filter(BlockColumn::Epoch.eq(208))
        .order_by_asc(BlockColumn::Id)
        .limit(256)
        .all(&conn)
        .await?;
    tracing::info!(
        "Shelley first blocks {:?}",
        shelley_first_blocks
            .iter()
            .map(|block| (block.id, block.height, block.epoch))
            .collect::<Vec<(i32, i32, i32)>>()
    );
    let shelley_first_blocks: Vec<i32> =
        shelley_first_blocks.iter().map(|block| block.id).collect();

    let mut condition = Condition::any();
    for block in shelley_first_blocks {
        condition = condition.add(TransactionColumn::BlockId.eq(block));
    }
    let shelley_first_tx: Vec<i64> = Transaction::find()
        .filter(condition)
        .order_by_asc(TransactionColumn::Id)
        .limit(1)
        .all(&conn)
        .await?
        .iter()
        .map(|tx| tx.id)
        .collect();
    let shelley_first_tx = shelley_first_tx
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("Can't find first tx"))?;
    tracing::info!("Shelley first tx, {:?}", shelley_first_tx);

    //////////////

    let transactions = Transaction::find()
        .filter(TransactionColumn::Id.gte(shelley_first_tx))
        .order_by_asc(TransactionColumn::Id)
        .paginate(&conn, config.tx_per_page as usize);
    let total_transactions = transactions.num_items().await?;
    let total_pages = transactions.num_pages().await?;
    tracing::info!("Total transactions: {:?}", total_transactions);
    tracing::info!("Total pages: {:?}", total_pages);

    let mut out_file = if config.events_output_path.exists() && config.events_output_path.is_file()
    {
        tracing::info!(
            "file {:?} already exists, adding lines to the end",
            config.events_output_path
        );
        File::open(config.events_output_path.clone())
    } else {
        File::create(config.events_output_path.clone())
    }?;

    let mut current_start = shelley_first_tx;
    let mut current_end = shelley_first_tx + config.tx_per_page;
    let max_end = shelley_first_tx + total_transactions as i64;

    let mut current_query = Transaction::find()
        .filter(
            Condition::all()
                .add(TransactionColumn::Id.gte(current_start))
                .add(TransactionColumn::Id.lt(current_end)),
        )
        .order_by_asc(TransactionColumn::Id)
        .all(&conn)
        .await?;

    let mut previous_outputs = HashMap::<String, HashMap<u64, TxOutput>>::new();

    let mut stake_address_to_num = DataMapper::<StakeCredential>::new();
    let mut payment_address_to_num = DataMapper::<StakeCredential>::new();
    let mut policy_id_to_num = DataMapper::<PolicyID>::new();
    let mut asset_name_to_num = DataMapper::<String>::new();
    let mut banned_addresses = HashSet::<(u64, Option<u64>)>::new();

    let mut unparsed_transactions = Vec::<TransactionModel>::new();
    let mut duplicated_inputs_txs: u64 = 0;

    while !current_query.is_empty() {
        let tx_count = current_query.len();
        tracing::info!(
            "fetched txs from {:?} to {:?}, total: {:?}, max: {:?}",
            current_start,
            current_end,
            tx_count,
            max_end
        );
        for tx in current_query {
            let payload: &Vec<u8> = &tx.payload;
            let tx_hash = hex::encode(tx.hash.clone());
            match cardano_multiplatform_lib::Transaction::from_bytes(payload.clone()) {
                Ok(parsed) => {
                    let body = parsed.body();
                    // inputs handle
                    let inputs = body.inputs();

                    let (has_banned_addresses, has_duplicated_inputs, input_events) =
                        match get_input_intents(
                            &tx_hash,
                            tx.id as u64,
                            inputs,
                            &mut previous_outputs,
                            &banned_addresses,
                        ) {
                            Ok(output) => output,
                            Err(err) => {
                                tracing::warn!(
                                    "error occurred while trying to get inputs: {:?}",
                                    err
                                );
                                unparsed_transactions.push(tx.clone());
                                continue;
                            }
                        };

                    if has_duplicated_inputs {
                        duplicated_inputs_txs += 1;
                    }

                    if has_banned_addresses {
                        ban_addresses_for_events(&input_events, &mut banned_addresses)?;
                    }

                    // outputs handle
                    let outputs = body.outputs();
                    let output_events = match get_output_intents(
                        &tx_hash,
                        outputs,
                        &mut previous_outputs,
                        &mut payment_address_to_num,
                        &mut stake_address_to_num,
                        &mut policy_id_to_num,
                        &mut asset_name_to_num,
                    ) {
                        Ok(result) => result,
                        Err(err) => {
                            tracing::warn!("error occurred while trying to get outputs: {:?}", err);
                            unparsed_transactions.push(tx.clone());
                            for input in input_events.into_iter() {
                                if let Some(addr) = input.address {
                                    banned_addresses.insert(addr);
                                }
                            }
                            continue;
                        }
                    };

                    let event = if has_banned_addresses {
                        let output_events: Vec<TxOutput> = output_events
                            .into_iter()
                            .filter(|output| {
                                !output.is_byron() && !output.is_banned(&banned_addresses)
                            })
                            .collect();
                        if output_events.is_empty() {
                            None
                        } else {
                            Some(TxEvent::Partial { to: output_events })
                        }
                    } else {
                        Some(TxEvent::Full {
                            to: output_events,
                            fee: dcspark_core::Value::<Regulated>::from(u64::from(body.fee())),
                            from: input_events,
                        })
                    };

                    if let Some(event) = event {
                        match &event {
                            TxEvent::Full { to, fee, from } => {
                                let mut input_value = dcspark_core::Value::zero();
                                let mut output_value = dcspark_core::Value::zero();
                                for to in to.iter() {
                                    output_value += &to.value;
                                }
                                output_value += fee;
                                for from in from.iter() {
                                    input_value += &from.value;
                                }
                                if input_value != output_value {
                                    for input in from.into_iter() {
                                        if let Some(addr) = input.address {
                                            banned_addresses.insert(addr);
                                        }
                                    }
                                    for output in to.into_iter() {
                                        if let Some(addr) = output.address {
                                            banned_addresses.insert(addr);
                                        }
                                    }
                                    continue;
                                }
                            }
                            TxEvent::Partial { .. } => {}
                        }
                        out_file.write_all(
                            format!("{}\n", serde_json::to_string(&event)?).as_bytes(),
                        )?;
                    }
                }
                Err(err) => {
                    tracing::warn!("Can't parse tx: {:?}, err: {:?}", tx_hash.clone(), err);
                    unparsed_transactions.push(tx.clone());
                }
            }
        }

        current_start = current_end;
        current_end += config.tx_per_page;
        current_end = min(current_end, max_end);
        current_query = Transaction::find()
            .filter(
                Condition::all()
                    .add(TransactionColumn::Id.gte(current_start))
                    .add(TransactionColumn::Id.lt(current_end)),
            )
            .order_by_asc(TransactionColumn::Id)
            .all(&conn)
            .await?;
    }

    drop(out_file);

    tracing::info!("Parsing finished, dumping files");
    tracing::info!(
        "Total unparsed transactions: {:?}",
        unparsed_transactions.len()
    );
    tracing::info!(
        "Total transactions with duplicated inputs: {:?}",
        duplicated_inputs_txs
    );

    dump_unparsed_transactions_to_file(config.unparsed_transactions, unparsed_transactions)?;

    payment_address_to_num.dump_to_file(config.payment_creds_mapping)?;
    stake_address_to_num.dump_to_file(config.staking_creds_mapping)?;
    policy_id_to_num.dump_to_file(config.policy_mapping)?;
    asset_name_to_num.dump_to_file(config.asset_name_mapping)?;
    dump_hashset_to_file(&banned_addresses, config.banned_addresses)?;

    tracing::info!("Dumping finished, cleaning events");

    clean_events(
        config.events_output_path,
        config.cleaned_events_output_path,
        &banned_addresses,
    )?;

    tracing::info!("Cleaning finished");

    Ok(())
}

fn dump_unparsed_transactions_to_file(
    path: PathBuf,
    txs: Vec<TransactionModel>,
) -> anyhow::Result<()> {
    let mut output = File::create(path)?;
    output.write_all(format!("{}\n", txs.len()).as_bytes())?;
    for tx in txs.into_iter() {
        output.write_all(format!("{}\n", serde_json::to_string(&tx)?).as_bytes())?;
    }
    Ok(())
}

fn clean_events(
    events_output_path: PathBuf,
    cleaned_events_output_path: PathBuf,
    banned_addresses: &HashSet<(u64, Option<u64>)>,
) -> anyhow::Result<()> {
    let file = File::open(events_output_path)?;
    let mut cleaned_file = File::create(cleaned_events_output_path)?;

    let reader = BufReader::new(file);
    let lines = reader.lines();
    for (num, line) in lines.enumerate() {
        let event: TxEvent = serde_json::from_str(line?.as_str())?;
        let event = match event {
            TxEvent::Partial { to } => {
                let to: Vec<TxOutput> = to
                    .into_iter()
                    .filter(|output| !output.is_byron() && !output.is_banned(&banned_addresses))
                    .collect();
                if !to.is_empty() {
                    Some(TxEvent::Partial { to })
                } else {
                    None
                }
            }
            TxEvent::Full { to, fee, from } => {
                if from
                    .iter()
                    .any(|input| input.is_byron() || input.is_banned(&banned_addresses))
                {
                    let new_to: Vec<TxOutput> = to
                        .into_iter()
                        .filter(|output| !output.is_byron() && !output.is_banned(&banned_addresses))
                        .collect();
                    if !new_to.is_empty() {
                        Some(TxEvent::Partial { to: new_to })
                    } else {
                        None
                    }
                } else {
                    let new_to: Vec<TxOutput> = to
                        .into_iter()
                        .map(|mut output| {
                            if output.is_banned(&banned_addresses) {
                                output.address = None;
                            }
                            output
                        })
                        .collect();
                    Some(TxEvent::Full {
                        to: new_to,
                        fee,
                        from,
                    })
                }
            }
        };
        if let Some(event) = event {
            cleaned_file.write_all(format!("{}\n", serde_json::to_string(&event)?).as_bytes())?;
        }
        if num % 100000 == 0 {
            tracing::info!("Processed {:?} entries", num + 1);
        }
    }

    Ok(())
}

fn get_input_intents(
    tx_hash: &String,
    tx_id: u64,
    inputs: cardano_multiplatform_lib::TransactionInputs,
    previous_outputs: &mut HashMap<String, HashMap<u64, TxOutput>>,
    banned_addresses: &HashSet<(u64, Option<u64>)>,
) -> anyhow::Result<(bool, bool, Vec<TxOutput>)> {
    let mut has_byron_inputs = false;

    // try to parse input addresses and put in the set
    let mut parsed_inputs = Vec::new();
    let mut inputs_pointers = HashSet::<(String, u64)>::new();
    let mut seen_tx_ids = Vec::new();

    let mut duplicated_pointers_found = false;

    for input_index in 0..inputs.len() {
        let input = inputs.get(input_index);
        let input_tx_id = input.transaction_id().to_hex();
        let input_tx_index = u64::from(input.index());

        // try to find output that is now used as an input
        if let Some(outputs) = &mut previous_outputs.get_mut(&input_tx_id) {
            // we remove the spent input from the list
            if let Some(output) = outputs.remove(&input_tx_index) {
                inputs_pointers.insert((input_tx_id.clone(), input_tx_index.clone()));
                parsed_inputs.push(output);
            } else {
                if inputs_pointers.contains(&(input_tx_id.clone(), input_tx_index.clone())) {
                    duplicated_pointers_found = true;
                    tracing::info!("Found tx using same output as an input multiple times: {:?}@{:?}, current tx: {:?}, id: {:?}",
                        input_tx_id,
                        input_tx_index,
                        tx_hash,
                        tx_id,
                    );
                    continue;
                }
                // invalid transaction
                tracing::warn!(
                    "Can't find matching output for used input: {:?}@{:?}, current tx: {:?}, id: {:?}",
                    input_tx_id,
                    input_tx_index,
                    tx_hash,
                    tx_id,
                );
                return Err(anyhow!(
                    "Can't find matching output for used input: {:?}@{:?}, current tx: {:?}, id: {:?}",
                    input.transaction_id().to_hex(),
                    input.index(),
                    tx_hash,
                    tx_id,
                ));
            }
        } else {
            has_byron_inputs = true; // might be byron address or sth
        }

        seen_tx_ids.push(input_tx_id);
    }

    for seen_id in seen_tx_ids {
        if previous_outputs
            .get(&seen_id)
            .map(|outputs| outputs.is_empty())
            .unwrap_or(false)
        {
            previous_outputs.remove(&seen_id);
        }
    }

    let has_banned_addresses = parsed_inputs.iter().any(|input| {
        input.address.is_none()
            || (input.address.is_some()
                && banned_addresses.contains(&input.address.clone().unwrap()))
    });

    Ok((
        has_byron_inputs || has_banned_addresses,
        duplicated_pointers_found,
        parsed_inputs,
    ))
}

fn get_output_intents(
    tx_hash: &String,
    outputs: cardano_multiplatform_lib::TransactionOutputs,
    previous_outputs: &mut HashMap<String, HashMap<u64, TxOutput>>,
    payment_address_mapping: &mut DataMapper<StakeCredential>,
    stake_address_mapping: &mut DataMapper<StakeCredential>,
    policy_to_num: &mut DataMapper<PolicyID>,
    asset_name_to_num: &mut DataMapper<String>,
) -> anyhow::Result<Vec<TxOutput>> {
    let mut parsed_outputs = Vec::new();
    for output_index in 0..outputs.len() {
        let output = outputs.get(output_index);

        let address = output.address();
        let address = match address.payment_cred() {
            None => {
                // this is byron output
                None
            }
            Some(payment) => {
                let payment_mapping = payment_address_mapping.add_if_not_presented(payment);
                let staking_mapping = address
                    .staking_cred()
                    .map(|staking| stake_address_mapping.add_if_not_presented(staking));
                Some((payment_mapping, staking_mapping))
            }
        };

        let amount = output.amount();
        let value = dcspark_core::Value::<Regulated>::from(u64::from(amount.coin()));
        let mut assets = Vec::new();

        if let Some(multiasset) = amount.multiasset() {
            let policy_ids = multiasset.keys();
            for policy_id_index in 0..policy_ids.len() {
                let policy_id = policy_ids.get(policy_id_index);
                if let Some(assets_by_policy_id) = multiasset.get(&policy_id) {
                    let asset_names = assets_by_policy_id.keys();
                    for asset_name_id in 0..asset_names.len() {
                        let asset_name = asset_names.get(asset_name_id);
                        let asset_value = assets_by_policy_id.get(&asset_name);
                        if let Some(asset_value) = asset_value {
                            let policy_mapping =
                                policy_to_num.add_if_not_presented(policy_id.clone());
                            let asset_name_mapping = asset_name_to_num
                                .add_if_not_presented(hex::encode(asset_name.name()));
                            assets.push(TxAsset {
                                asset_id: (policy_mapping, asset_name_mapping),
                                value: dcspark_core::Value::<Regulated>::from(u64::from(
                                    asset_value,
                                )),
                            })
                        }
                    }
                }
            }
        }

        parsed_outputs.push(TxOutput {
            address,
            value,
            assets,
        })
    }

    let entry = previous_outputs.entry(tx_hash.clone()).or_default();
    for (output_index, parsed_output) in parsed_outputs.iter().enumerate() {
        entry.insert(output_index as u64, parsed_output.clone());
    }

    Ok(parsed_outputs)
}

fn ban_addresses_for_events(
    events: &Vec<TxOutput>,
    banned_addresses: &mut HashSet<(u64, Option<u64>)>,
) -> anyhow::Result<()> {
    for event in events.iter() {
        if let Some((payment, staking)) = event.address {
            banned_addresses.insert((payment, staking));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cardano_multiplatform_lib::crypto::TransactionHash;

    #[test]
    fn test() {
        let tx_hash = TransactionHash::from_bytes(vec![
            101, 106, 250, 127, 137, 49, 211, 112, 238, 220, 189, 229, 84, 138, 171, 84, 242, 131,
            186, 7, 51, 239, 48, 123, 135, 235, 45, 50, 19, 86, 67, 142,
        ])
        .unwrap();
        println!("{}", tx_hash.to_hex());
    }
}
