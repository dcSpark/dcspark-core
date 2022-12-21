use crate::policy_id::PolicyId;
use crate::{AssetName, Regulated, TokenId, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TransactionAsset {
    pub policy_id: PolicyId,
    pub asset_name: AssetName,
    pub fingerprint: TokenId,
    pub quantity: Value<Regulated>,
}
