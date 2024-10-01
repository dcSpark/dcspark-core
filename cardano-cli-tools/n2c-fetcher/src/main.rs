use clap::Parser;
use dcspark_blockchain_source::cardano::{N2CSource, Point, Relay};
use dcspark_blockchain_source::Source;
use dcspark_core::{BlockId, SlotNumber};

#[derive(Parser, Debug)]
#[clap(version)]
struct Cli {
    #[clap(long, value_parser, default_value = "mainnet")]
    pub network: String,
    #[clap(long, value_parser)]
    pub since: Option<String>,
    #[clap(long, value_parser)]
    pub unix_socket: String,
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
        unix_socket,
    } = Cli::parse();

    let mut base_config = match network.as_ref() {
        "mainnet" => dcspark_blockchain_source::cardano::NetworkConfiguration::mainnet(),
        "preprod" => dcspark_blockchain_source::cardano::NetworkConfiguration::preprod(),
        "preview" => dcspark_blockchain_source::cardano::NetworkConfiguration::preview(),
        "sancho" => dcspark_blockchain_source::cardano::NetworkConfiguration::sancho(),
        _ => return Err(anyhow::anyhow!("network not supported by source")),
    };

    base_config.relay = Relay::UnixSocket(unix_socket);

    let start_from = match since.map(parse_since).transpose()? {
        Some((hash, slot)) => vec![Point::BlockHeader {
            slot_nb: slot,
            hash,
        }],
        None => vec![],
    };

    let mut source = N2CSource::connect(base_config, start_from).await?;

    while let Ok(Some(block)) = source.pull(&()).await {
        println!("{:?}", block);
    }

    Ok(())
}
