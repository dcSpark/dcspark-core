use anyhow::{anyhow, bail, Context};
use cardano_multiplatform_lib::{
    address::{BaseAddress, StakeCredential},
    crypto::Bip32PublicKey,
};
use reqwest::{blocking::Client, header::CONTENT_TYPE};
use std::str::FromStr;
use structopt::StructOpt;

const STAKING_KEY_INDEX: u32 = 0;
const EXTERNAL: u32 = 0;
const CHIMERIC_ACCOUNT_DERIVATION: u32 = 2;

#[derive(Debug)]
#[repr(u8)]
enum NetworkId {
    Testnet = 0,
    Mainnet = 1,
}

impl FromStr for NetworkId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(NetworkId::Mainnet),
            "testnet" => Ok(NetworkId::Testnet),
            _ => bail!("Invalid network id. Should be either mainnet or testnet."),
        }
    }
}

impl std::fmt::Display for NetworkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let as_str = match self {
            NetworkId::Testnet => "testnet",
            NetworkId::Mainnet => "mainnet",
        };

        write!(f, "{as_str}")
    }
}

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(short, long)]
    public_key: String,

    #[structopt(short, long)]
    network: NetworkId,
}

fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::from_args();

    let carp_base_url = format!("https://gate.flint-wallet.com/{}/carp", opt.network);

    let pk = hex::decode(opt.public_key)
        .context("public key should be a valid hex string")
        .and_then(|bytes| {
            Bip32PublicKey::from_bytes(&bytes).map_err(|_| anyhow!("invalid public key"))
        })?;

    let staking_key = pk
        .derive(CHIMERIC_ACCOUNT_DERIVATION)
        .and_then(|pk| pk.derive(STAKING_KEY_INDEX))
        .map_err(|e| anyhow!("couldn't derive staking key. Reason {e}"))?;

    let spending = pk
        .derive(EXTERNAL)
        .map_err(|e| anyhow!("couldn't derive external tree: {e}"))?
        .derive(0)
        .map_err(|e| anyhow!("couldn't derive first address: {e}"))?;

    let base_address = BaseAddress::new(
        opt.network as u8,
        &StakeCredential::from_keyhash(&spending.to_raw_key().hash()),
        &StakeCredential::from_keyhash(&staking_key.to_raw_key().hash()),
    );

    println!(
        "checking backend for address:\n {}",
        base_address.to_address().to_bech32(None).unwrap()
    );

    let client = Client::new();

    let latest = client
        .post(format!("{carp_base_url}{}", "/block/latest"))
        .header(CONTENT_TYPE, "application/json")
        .body(r#"{"offset": 0}"#)
        .send()
        .context("couldn't send /block/latest request")?;

    let latest: BlockLatestResponse =
        miniserde::json::from_str(&latest.text().context("failed to latest block")?)
            .context("couldn't parse /block/latest response")?;

    let result = client
        .post(format!("{carp_base_url}{}", "/address/used"))
        .header(CONTENT_TYPE, "application/json")
        .body(miniserde::json::to_string(&AddressUsed {
            addresses: vec![base_address.to_address().to_bech32(None).unwrap()],
            until_block: latest.block.hash,
        }))
        .send()
        .context("couldn't send request to /address/used")?;

    if !result.status().is_success() {
        bail!("error checking in the backend if the address is used");
    }

    let result = miniserde::json::from_str::<AddressUsedResult>(&result.text().unwrap())
        .context("couldn't parse /address/used response")?;

    if !result.addresses.is_empty() {
        println!("result:\n used");
    } else {
        println!("result:\n unused");
    }

    Ok(())
}

#[derive(miniserde::Serialize, miniserde::Deserialize, Debug)]
struct AddressUsed {
    addresses: Vec<String>,
    #[serde(rename = "untilBlock")]
    until_block: String,
}

#[derive(miniserde::Serialize, miniserde::Deserialize, Debug)]
struct AddressUsedAfter {
    tx: String,
    block: String,
}

#[derive(miniserde::Serialize, miniserde::Deserialize, Debug)]
struct AddressUsedResult {
    addresses: Vec<String>,
}

#[derive(miniserde::Serialize, miniserde::Deserialize, Debug)]
struct BlockLatestResponse {
    block: BlockLatestResponseBlock,
}

#[derive(miniserde::Serialize, miniserde::Deserialize, Debug)]
struct BlockLatestResponseBlock {
    era: u64,
    hash: String,
    height: u64,
    epoch: u64,
    slot: u64,
}
