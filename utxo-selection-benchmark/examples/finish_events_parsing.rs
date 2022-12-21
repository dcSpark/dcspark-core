use anyhow::{anyhow, Context};
use cardano_multiplatform_lib::address::{Address, StakeCredential};
use cardano_multiplatform_lib::error::JsError;
use clap::Parser;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use utxo_selection_benchmark::mapper::DataMapper;
use utxo_selection_benchmark::tx_event::{TxEvent, TxOutput};
use utxo_selection_benchmark::utils::{
    dump_hashmap_to_file, dump_hashset_to_file, read_hashmap_from_file, read_hashset_from_file,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    events_path: PathBuf,
    cleaned_events_output_path: PathBuf,

    unparsed_transaction_addresses: PathBuf,

    payment_creds_mapping: PathBuf,
    payment_creds_mapping_output: PathBuf,
    staking_creds_mapping: PathBuf,
    staking_creds_mapping_output: PathBuf,
    address_to_mapping: PathBuf,
    address_to_mapping_output: PathBuf,
    banned_addresses: PathBuf,
    banned_addresses_output: PathBuf,
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

    tracing_subscriber::registry().with(fmt_layer).init();

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

    let mut unparsed_addresses_file = if config.unparsed_transaction_addresses.exists()
        && config.unparsed_transaction_addresses.is_file()
    {
        File::open(config.unparsed_transaction_addresses.clone())?
    } else {
        return Err(anyhow!(
            "can't open input file: {:?}",
            config.unparsed_transaction_addresses.clone()
        ));
    };

    tracing::info!("loading mappings");

    let mut stake_address_to_num =
        DataMapper::<StakeCredential>::load_from_file(config.staking_creds_mapping)?;
    tracing::info!("stake addresses loaded");

    let mut payment_address_to_num =
        DataMapper::<StakeCredential>::load_from_file(config.payment_creds_mapping)?;
    tracing::info!("payment addresses loaded");

    let mut banned_addresses: HashSet<(u64, Option<u64>)> =
        read_hashset_from_file(config.banned_addresses)?;
    tracing::info!("banned addresses loaded");

    let mut address_to_mapping: HashMap<String, (u64, Option<u64>)> =
        read_hashmap_from_file(config.address_to_mapping)?;
    tracing::info!("address mapping loaded");

    tracing::info!("successfully loaded mappings");

    let unparsed_addresses_file_lines = BufReader::new(unparsed_addresses_file).lines();
    for line in unparsed_addresses_file_lines {
        let address = line?;
        match cardano_multiplatform_lib::address::Address::from_bech32(address.as_str()) {
            Ok(address) => match address.payment_cred() {
                None => {
                    // this is byron output
                }
                Some(payment) => {
                    let payment_mapping = payment_address_to_num.add_if_not_presented(payment);
                    let staking_mapping = address
                        .staking_cred()
                        .map(|staking| stake_address_to_num.add_if_not_presented(staking));
                    address_to_mapping.insert(
                        address
                            .to_bech32(None)
                            .map_err(|err| anyhow!("Can't convert address to bech32: {:?}", err))?,
                        (payment_mapping, staking_mapping),
                    );
                    banned_addresses.insert((payment_mapping, staking_mapping));
                }
            },
            Err(err) => {
                tracing::error!("can't parse address: {:?}, addr={:?}", err, address);
            }
        }
    }

    tracing::info!("Parsing finished, dumping files");

    payment_address_to_num.dump_to_file(config.payment_creds_mapping_output)?;
    stake_address_to_num.dump_to_file(config.staking_creds_mapping_output)?;
    dump_hashmap_to_file(&address_to_mapping, config.address_to_mapping_output)?;
    dump_hashset_to_file(&banned_addresses, config.banned_addresses_output)?;

    tracing::info!("Dumping finished, cleaning events");

    clean_events(
        config.events_path,
        config.cleaned_events_output_path,
        &banned_addresses,
    )?;

    tracing::info!("Cleaning finished");

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
            TxEvent::Full { mut to, fee, from } => {
                if from
                    .iter()
                    .any(|input| input.is_byron() || input.is_banned(&banned_addresses))
                {
                    to = to
                        .into_iter()
                        .filter(|output| !output.is_byron() && !output.is_banned(&banned_addresses))
                        .collect();
                    if !to.is_empty() {
                        Some(TxEvent::Partial { to })
                    } else {
                        None
                    }
                } else {
                    to = to
                        .into_iter()
                        .map(|mut output| {
                            if output.is_banned(&banned_addresses) {
                                output.address = None;
                            }
                            output
                        })
                        .collect();
                    Some(TxEvent::Full { to, fee, from })
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
