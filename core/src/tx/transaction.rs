use crate::tx::{TransactionId, UTxODetails, UtxoPointer};
use deps::serde_json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// a block content as defined in the blockchain
///
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub transaction_id: TransactionId,

    pub inputs: Vec<UtxoPointer>,
    pub outputs: Vec<UTxODetails>,

    #[serde(default)]
    pub metadata: Arc<serde_json::Value>,
}
