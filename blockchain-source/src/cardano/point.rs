use anyhow::anyhow;
use dcspark_core::{BlockId, SlotNumber};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Point {
    Origin,
    BlockHeader { slot_nb: SlotNumber, hash: BlockId },
}

impl TryFrom<Point> for cardano_sdk::protocol::Point {
    type Error = anyhow::Error;

    fn try_from(point: Point) -> anyhow::Result<Self> {
        match point {
            Point::Origin => Ok(cardano_sdk::protocol::Point::ORIGIN),
            Point::BlockHeader { slot_nb, hash } => {
                cardano_sdk::protocol::Point::from_raw(slot_nb.into(), hash.as_ref())
                    .ok_or_else(|| anyhow! {"invalid block id {}", hash})
            }
        }
    }
}

impl From<cardano_sdk::protocol::Point> for Point {
    fn from(point: cardano_sdk::protocol::Point) -> Self {
        if point == cardano_sdk::protocol::Point::ORIGIN {
            Point::Origin
        } else {
            Point::BlockHeader {
                slot_nb: SlotNumber::new(point.slot_nb()),
                hash: BlockId::new(point.hash().to_string()),
            }
        }
    }
}
