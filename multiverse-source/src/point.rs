use anyhow::Result;
use dcspark_blockchain_source::{
    cardano::{BlockEvent, CardanoNetworkEvent, Tip},
    PullFrom,
};

pub trait Point: PullFrom + Sized + Clone + Send + Sync {
    type V;

    // A function that allows the multiverse to extract the From argument from the Event of the
    // wrapped source.
    //
    // For Cardano, this allows extracting the SlotNumber + BlockId from the BlockEvent, in order to be able
    // to construct a Point.
    //
    // This is needed because Point can't be the key of the Multiverse, since it is not really
    // possible to compute the parent of a Point, since the SlotNumber of the parent could be
    // anything.
    fn from_multiverse_entry(v: &Self::V) -> Result<Self>;
}

impl Point for dcspark_blockchain_source::cardano::Point {
    type V = CardanoNetworkEvent<BlockEvent, Tip>;

    fn from_multiverse_entry(v: &Self::V) -> anyhow::Result<Self> {
        match v {
            CardanoNetworkEvent::Tip(_) => {
                unreachable!("tip event shouldn't be inserted in the multiverse")
            }
            CardanoNetworkEvent::Block(v) => {
                Ok(dcspark_blockchain_source::cardano::Point::BlockHeader {
                    slot_nb: v.slot_number,
                    hash: v.id.clone(),
                })
            }
        }
    }
}
