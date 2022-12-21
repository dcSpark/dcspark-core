use dcspark_core::tx::TransactionAsset;
use dcspark_core::{Address, AssetName, PolicyId, Regulated, TokenId};
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::collections::HashSet;
use std::str::FromStr;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxAsset {
    #[serde(rename = "aid")]
    pub asset_id: (u64, u64),
    #[serde(rename = "val")]
    pub value: dcspark_core::Value<Regulated>,
}

impl From<TxAsset> for TransactionAsset {
    fn from(asset: TxAsset) -> Self {
        TransactionAsset {
            policy_id: PolicyId::new(asset.asset_id.0.to_string()),
            asset_name: AssetName::new(asset.asset_id.1.to_string()),
            fingerprint: TokenId::new(format!("{}_{}", asset.asset_id.0, asset.asset_id.1)),
            quantity: asset.value.clone(),
        }
    }
}

pub fn address_from_pair(address: (u64, Option<u64>)) -> Address {
    Address::new(format!("{}_{:?}", address.0, address.1))
}

pub fn pair_from_address(address: Address) -> Option<(u64, Option<u64>)> {
    if address.as_ref() == "" {
        return None;
    }
    let split = address
        .split("_")
        .into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    if split.len() != 2 {
        return None;
    }
    let payment = match split
        .get(0)
        .map(|payment| u64::from_str(payment.as_str()).ok())
        .flatten()
    {
        None => return None,
        Some(payment) => payment,
    };
    let staking = match split.get(0).map(|staking| {
        let staking: Option<u64> = serde_json::from_str(staking.as_str()).ok().flatten();
        staking
    }) {
        None => return None,
        Some(staking) => staking,
    };
    Some((payment, staking))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxOutput {
    #[serde(rename = "addr")]
    pub address: Option<(u64, Option<u64>)>,
    #[serde(rename = "val")]
    pub value: dcspark_core::Value<Regulated>,
    #[serde(rename = "ass")]
    pub assets: Vec<TxAsset>,
}

impl TxOutput {
    pub fn is_banned(&self, banned_addresses: &HashSet<(u64, Option<u64>)>) -> bool {
        self.address
            .map(|address| banned_addresses.contains(&address))
            .unwrap_or(false)
    }

    pub fn is_byron(&self) -> bool {
        self.address.is_none()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum TxEvent {
    Full {
        to: Vec<TxOutput>,
        fee: dcspark_core::Value<Regulated>,
        from: Vec<TxOutput>,
    },
    Partial {
        to: Vec<TxOutput>,
    },
}
