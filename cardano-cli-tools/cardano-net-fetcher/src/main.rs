use clap::Parser;

use dcspark_blockchain_source::cardano::Point::{BlockHeader, Origin};
use dcspark_blockchain_source::cardano::{CardanoNetworkEvent, CardanoSource};

use dcspark_blockchain_source::{GetNextFrom, Source};
use dcspark_core::{BlockId, SlotNumber};
use std::borrow::Cow;

use std::time::Duration;

#[derive(Parser, Debug)]
#[clap(version)]
struct Cli {
    #[clap(long, value_parser, default_value = "mainnet")]
    pub network: String,
    #[clap(long, value_parser)]
    pub since: Option<String>,
    #[clap(long, value_parser)]
    pub relay_host: String,
    #[clap(long, value_parser)]
    pub relay_port: u16,
}

fn parse_since(since: String) -> anyhow::Result<(BlockId, SlotNumber)> {
    let mut parts: Vec<_> = since.split(',').collect();
    let slot: SlotNumber = SlotNumber::new(parts.remove(0).parse()?);
    let hash: BlockId = BlockId::new(parts.remove(0).to_owned());
    Ok((hash, slot))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Cli {
        network,
        since,
        relay_host,
        relay_port,
    } = Cli::parse();

    let base_config = match network.as_ref() {
        "mainnet" => dcspark_blockchain_source::cardano::NetworkConfiguration::mainnet(),
        "preprod" => dcspark_blockchain_source::cardano::NetworkConfiguration::preprod(),
        "preview" => dcspark_blockchain_source::cardano::NetworkConfiguration::preview(),
        _ => return Err(anyhow::anyhow!("network not supported by source")),
    };

    let from = match since {
        None => Origin,
        Some(since) => {
            let (since_hash, since_slot) = parse_since(since)?;
            BlockHeader {
                slot_nb: since_slot,
                hash: since_hash,
            }
        }
    };

    let network_config = dcspark_blockchain_source::cardano::NetworkConfiguration {
        relay: (Cow::from(relay_host), relay_port),
        from: from.clone(),
        ..base_config
    };

    let mut source = CardanoSource::connect(&network_config, Duration::from_secs(20)).await?;

    let mut pull_from = from;

    while let Some(event) = source.pull(&vec![pull_from.clone()]).await? {
        let block = match &event {
            CardanoNetworkEvent::Tip(_) => continue,
            CardanoNetworkEvent::Block(block) => block.clone(),
        };

        let new_from = event.next_from().unwrap_or(pull_from.clone());
        pull_from = new_from;

        println!(
            "Block #{}, point: {}@{}, raw cbor hex: {}",
            block.block_number,
            block.id,
            block.slot_number,
            hex::encode(block.raw_block),
        );
    }

    Ok(())
}
