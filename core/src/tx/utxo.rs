use crate::tx::{TransactionAsset, TransactionId};
use crate::{Address, OutputIndex, Regulated, Value};

use deps::serde_json;
use serde::{Deserialize, Serialize};

use std::fmt;
use std::sync::Arc;

/// Points to particular UTxO for some ['TransactionId'].
/// We can have multiple pointers with different indexes for the same transaction.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct UtxoPointer {
    pub transaction_id: TransactionId,
    pub output_index: OutputIndex,
}

impl fmt::Display for UtxoPointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{hash}@{index}",
            hash = self.transaction_id,
            index = self.output_index,
        )
    }
}

/// list the details of the UTxO
///
/// this is the information that we will collect int he UTxO store
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct UTxODetails {
    pub pointer: UtxoPointer,
    pub address: Address,
    pub value: Value<Regulated>,

    #[serde(default)]
    pub assets: Vec<TransactionAsset>,
    pub metadata: Arc<serde_json::Value>,

    #[serde(default)]
    pub extra: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct UTxOBuilder {
    pub address: Address,
    pub value: Value<Regulated>,
    pub assets: Vec<TransactionAsset>,

    #[serde(default)]
    pub extra: Option<String>,
}

impl UTxOBuilder {
    pub fn new(address: Address, value: Value<Regulated>, assets: Vec<TransactionAsset>) -> Self {
        Self {
            address,
            value,
            assets,
            extra: None,
        }
    }
}
