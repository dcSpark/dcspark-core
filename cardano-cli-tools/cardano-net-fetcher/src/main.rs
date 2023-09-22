use clap::Parser;
use dcspark_blockchain_source::cardano::Point::BlockHeader;
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
        "sancho" => dcspark_blockchain_source::cardano::NetworkConfiguration::sancho(),
        _ => return Err(anyhow::anyhow!("network not supported by source")),
    };

    let mut pull_from = match since {
        None => vec![],
        Some(since) => {
            let (since_hash, since_slot) = parse_since(since)?;
            vec![BlockHeader {
                slot_nb: since_slot,
                hash: since_hash,
            }]
        }
    };

    let network_config = dcspark_blockchain_source::cardano::NetworkConfiguration {
        relay: (Cow::from(relay_host), relay_port),
        ..base_config
    };

    let mut source = CardanoSource::connect(&network_config, Duration::from_secs(20)).await?;

    while let Some(event) = source.pull(&pull_from).await? {
        let block = match &event {
            CardanoNetworkEvent::Tip(_) => continue,
            CardanoNetworkEvent::Block(block) => block.clone(),
        };

        pull_from = event
            .next_from()
            .map(|point| vec![point])
            .unwrap_or(pull_from.clone());

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
