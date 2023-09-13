use anyhow::anyhow;
use clap::Parser;
use oura::filters::selection;
use oura::filters::selection::Predicate;
use oura::mapper;
use oura::mapper::ChainWellKnownInfo;
use oura::model::EventData;
use oura::pipelining::{FilterProvider, SourceProvider};
use oura::sources::{n2c, n2n, AddressArg, BearerKind, IntersectArg, MagicArg, PointArg};
use oura::utils::{Utils, WithUtils};
use std::str::FromStr;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[clap(version)]
struct Cli {
    #[clap(long, value_parser, default_value = "mainnet")]
    pub magic: String,
    #[clap(long, value_parser)]
    pub bearer: BearerKind,
    #[clap(long, value_parser)]
    pub since: Option<String>,
    #[clap(long, value_parser)]
    pub socket: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        magic,
        bearer,
        socket,
        since,
    } = Cli::parse();

    let magic = MagicArg::from_str(&magic).map_err(|_| anyhow!("magic arg failed"))?;

    let well_known = match magic.0 {
        4 => ChainWellKnownInfo {
            byron_epoch_length: 86400,
            byron_slot_length: 20,
            byron_known_slot: 0,
            byron_known_hash: "".to_string(),
            byron_known_time: 1686789000,
            shelley_epoch_length: 86400,
            shelley_slot_length: 1,
            shelley_known_slot: 0,
            shelley_known_hash: "".to_string(),
            shelley_known_time: 1686789000,
            address_hrp: "addr_test".to_string(),
            adahandle_policy: "".to_string(),
        },
        _ => ChainWellKnownInfo::try_from_magic(*magic)
            .map_err(|_| anyhow!("chain well known info failed"))?,
    };

    let utils = Arc::new(Utils::new(well_known));

    let mapper = mapper::Config {
        include_transaction_details: true,
        include_block_cbor: true,
        ..Default::default()
    };

    let since = match since {
        None => IntersectArg::Origin,
        Some(string) => IntersectArg::Point(
            PointArg::from_str(&string).map_err(|err| anyhow!("can't parse point: {err}"))?,
        ),
    };
    let since = Some(since);

    let (source_handle, source_rx) = match bearer {
        #[allow(deprecated)]
        BearerKind::Unix => {
            let source_config = n2c::Config {
                address: AddressArg(BearerKind::Unix, socket),
                magic: Some(magic),
                well_known: None,
                mapper,
                since: None,
                min_depth: 0,
                intersect: since,
                retry_policy: None,
                finalize: None, // TODO: configurable
            };
            WithUtils::new(source_config, utils).bootstrap()
        }
        #[allow(deprecated)]
        BearerKind::Tcp => {
            let source_config = n2n::Config {
                address: AddressArg(BearerKind::Tcp, socket),
                magic: Some(magic),
                well_known: None,
                mapper,
                since: None,
                min_depth: 0,
                intersect: since,
                retry_policy: None,
                finalize: None, // TODO: configurable
            };
            WithUtils::new(source_config, utils).bootstrap()
        }
    }
    .map_err(|e| {
        anyhow!("failed to bootstrap source: {e}. Are you sure cardano-node is running?")
    })?;

    let mut handles = Vec::new();
    handles.push(source_handle);

    let check = Predicate::VariantIn(vec![String::from("Block"), String::from("Rollback")]);

    let filter_setup = selection::Config { check };

    let (filter_handle, filter_rx) = filter_setup
        .bootstrap(source_rx)
        .map_err(|_| anyhow!("failed to bootstrap filter"))?;

    handles.push(filter_handle);

    for input in filter_rx.into_iter() {
        if let EventData::Block(block_record) = input.data {
            let cbor = block_record
                .cbor_hex
                .ok_or_else(|| anyhow!("cbor is not presented"))?;
            println!(
                "Block #{}, point: {}@{}, raw cbor hex: {}",
                block_record.number, block_record.hash, block_record.slot, cbor
            );
        }
    }

    Ok(())
}
