use dcspark_core::{BlockId, BlockNumber, Timestamp, TransactionId};
use serde::{Deserialize, Serialize};

/// the unique identifier of an Event
///
/// Now a transaction may raise multiple events at a time. For example
///
/// 1. WrappingVotedOn: adding a new vote on a given wrapping request
/// 2. WrappingSuccess: because the vote reached the quorum
///
/// The index should reflect the order the event are emitted within
/// the transaction execution flow.
///
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub struct EventId {
    /// the transaction unique identifier within the blockchain that
    /// resulted to the given event.
    pub transaction: TransactionId,
    /// the index of the event within the transaction
    pub index: u64,
}

/// an event as recorded on chain
///
/// the [`EventInfo`] records all the necessary information to retrieve
/// the event within the blockchain.
///
/// the `EXTRA` can be used to add additional information, for example
/// for cardano the `EXTRA` can be a `UTxOPointer`.
///
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Event<EVENT, EXTRA = ()> {
    pub info: EventInfo,
    pub event: EVENT,
    pub extra: EXTRA,
}

/// All the necessary information to retrieve the event within the blockchain.
///
/// Now it is possible some information are not available or are not relevant.
/// For example it may be that the [`BlockId`] is left to `"N/A"` on
/// Algorand blockchain because we have instant finality and knowing
/// the [`BlockNumber`] (the `Round`) is enough to identify the block
/// where the event is located.
///
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct EventInfo {
    pub block_id: BlockId,
    pub block_number: BlockNumber,
    // this is the time the block was added on chain
    pub block_timestamp: Timestamp,
    pub parent_block_id: BlockId,
    pub event_id: EventId,
}
