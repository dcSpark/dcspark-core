use anyhow::Result;
use async_trait::async_trait;
use dcspark_core::{BlockId, BlockNumber, TransactionId};

/// Trait that defines how a we are to handle a source of event.
///
/// there are two main functions to implement:
///
/// - `pull` that may pause and fetch a transaction from the node.
///   it is possible the transactions will be fetched in batches.
///   if that is the case then it is also needed to implement the
///   Source::clear_buffers function.
///   The `From` parameters is used to let know the sourcer from
///   which point we are interested to pull from.
///
/// Now it may be that some other kind of handling needs to happen
/// between the processing of the events received from the source
/// and the executor: for example we might need to have the
/// multiverse performing the stable buffering in between. We might
/// also have the event_storage in between so that the events are
/// persistently stored for future usage (sharing or fast re-sync).
///
/// Example for algorand we might want to wrap as follow:
///
/// ```no_compile
/// type Source = EventStorage<AlgorandSource>;
/// ```
///
/// For Cardano we will want to use something more like:
///
/// ```no_compile
/// type Source = EventStorage<Multiverse<CardanoSource>>;
/// ```
///
/// This is a much leaner approach to the handling of blocks and events
/// as we are not providing a standardised interface on how to do the
/// event sourcing. Before this the Engine would have to manage how the
/// different blockchain works and how they are using the multiverse
///
#[async_trait]
pub trait Source {
    /// The event that the `Source` will allow us to get
    type Event: EventObject;
    /// when pulling from the `Source` we set this value so the `Source`
    /// knows how we don't care about previous events.
    type From: PullFrom;

    /// Pull event from the source.
    async fn pull(&mut self, from: &Self::From) -> Result<Option<Self::Event>>;
}

pub trait EventObject: Send {
    fn is_blockchain_tip(&self) -> bool;
}

pub trait PullFrom: Send {
    //
}

impl PullFrom for BlockNumber {}
impl PullFrom for BlockId {}
impl PullFrom for TransactionId {}

impl<T: PullFrom> PullFrom for Option<T> {}
