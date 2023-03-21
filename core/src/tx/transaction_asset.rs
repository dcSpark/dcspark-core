use crate::policy_id::PolicyId;
use crate::{AssetName, Regulated, TokenId, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TransactionAsset {
    pub policy_id: PolicyId,
    pub asset_name: AssetName,
    pub fingerprint: TokenId,
    pub quantity: Value<Regulated>,
}

impl TransactionAsset {
    pub fn new(
        policy_id: PolicyId,
        asset_name: AssetName,
        fingerprint: TokenId,
    ) -> TransactionAsset {
        Self {
            policy_id,
            asset_name,
            fingerprint,
            quantity: Default::default(),
        }
    }
}
