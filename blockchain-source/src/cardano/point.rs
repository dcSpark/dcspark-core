use anyhow::Context as _;
use dcspark_core::{BlockId, SlotNumber};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
pub enum Point {
    Origin,
    BlockHeader { slot_nb: SlotNumber, hash: BlockId },
}

impl TryFrom<Point> for pallas_network::miniprotocols::Point {
    type Error = anyhow::Error;

    fn try_from(point: Point) -> anyhow::Result<Self> {
        match point {
            Point::Origin => Ok(pallas_network::miniprotocols::Point::Origin),
            Point::BlockHeader { slot_nb, hash } => {
                Ok(pallas_network::miniprotocols::Point::Specific(
                    slot_nb.into(),
                    hex::decode(hash).context("invalid block hash")?,
                ))
            }
        }
    }
}

impl Point {
    pub fn slot_nb(&self) -> SlotNumber {
        match self {
            Point::Origin => SlotNumber::from(0),
            Point::BlockHeader { slot_nb, hash: _ } => slot_nb.clone(),
        }
    }
}
