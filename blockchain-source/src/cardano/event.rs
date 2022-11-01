use crate::{EventObject, GetNextFrom};
use anyhow::Context;
use cardano_sdk::protocol::SerializedBlock;
use dcspark_core::{BlockId, BlockNumber, SlotNumber};
use serde::de::Visitor;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CardanoNetworkEvent<Block, Tip> {
    #[serde(skip)]
    Tip(Tip),
    Block(Block),
}

impl<Block: Send, Tip: Send> EventObject for CardanoNetworkEvent<Block, Tip> {
    fn is_blockchain_tip(&self) -> bool {
        matches!(self, CardanoNetworkEvent::Tip(_))
    }
}

#[derive(Debug, Clone)]
pub struct BlockEvent {
    pub id: BlockId,
    pub parent_id: BlockId,
    pub block_number: BlockNumber,
    pub raw_block: SerializedBlock,
    pub slot_number: SlotNumber,
}

impl<Block, Tip> CardanoNetworkEvent<Block, Tip> {
    pub fn map_block<MappedBlock>(
        self,
        f: impl Fn(Block) -> anyhow::Result<MappedBlock>,
    ) -> anyhow::Result<CardanoNetworkEvent<MappedBlock, Tip>> {
        match self {
            CardanoNetworkEvent::Tip(tip) => Ok(CardanoNetworkEvent::Tip(tip)),
            CardanoNetworkEvent::Block(block) => f(block).map(CardanoNetworkEvent::Block),
        }
    }

    pub fn map_tip<MappedTip>(
        self,
        f: impl Fn(Tip) -> anyhow::Result<MappedTip>,
    ) -> anyhow::Result<CardanoNetworkEvent<Block, MappedTip>> {
        match self {
            CardanoNetworkEvent::Block(block) => Ok(CardanoNetworkEvent::Block(block)),
            CardanoNetworkEvent::Tip(tip) => f(tip).map(CardanoNetworkEvent::Tip),
        }
    }
}

impl serde::Serialize for BlockEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.raw_block.as_ref())
    }
}

impl<'de> serde::Deserialize<'de> for BlockEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct V;

        impl<'de> Visitor<'de> for V {
            type Value = BlockEvent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a byte array")
            }

            fn visit_bytes<E>(self, buf: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let mut reader = cbored::Reader::new(buf);

                let raw_block: SerializedBlock = reader
                    .decode()
                    .context("invalid block")
                    .map_err(serde::de::Error::custom)?;

                let block = raw_block
                    .unserialize()
                    .context("Couldn't deserialize block")
                    .map_err(serde::de::Error::custom)?;

                let header = block.header();
                let parent_id = get_parent_id(&header);

                Ok(BlockEvent {
                    id: BlockId::new(header.hash().to_string()),
                    parent_id,
                    block_number: BlockNumber::new(header.block_number()),
                    raw_block,
                    slot_number: SlotNumber::new(header.slot()),
                })
            }
        }

        deserializer.deserialize_bytes(V)
    }
}

pub(crate) fn get_parent_id(header: &cardano_sdk::chain::Header) -> BlockId {
    header
        .prev_hash()
        .option_ref()
        .map(|id| BlockId::new(id.to_string()))
        .unwrap_or_else(|| BlockId::new_static("0x0000000000000000000000000000000000000000"))
}

impl<Tip> multiverse::Variant for CardanoNetworkEvent<BlockEvent, Tip> {
    type Key = BlockId;

    fn id(&self) -> &Self::Key {
        match self {
            CardanoNetworkEvent::Tip(_) => {
                unreachable!("the tip event shouldn't be inserted in the multiverse")
            }
            CardanoNetworkEvent::Block(block) => &block.id,
        }
    }

    fn parent_id(&self) -> &Self::Key {
        match self {
            CardanoNetworkEvent::Tip(_) => {
                unreachable!("the tip event shouldn't be inserted in the multiverse")
            }
            CardanoNetworkEvent::Block(block) => &block.parent_id,
        }
    }

    fn block_number(&self) -> dcspark_core::BlockNumber {
        match self {
            CardanoNetworkEvent::Tip(_) => {
                unreachable!("the tip event shouldn't be inserted in the multiverse")
            }
            CardanoNetworkEvent::Block(block) => block.block_number.into_inner().into(),
        }
    }
}

impl<Tip> GetNextFrom for CardanoNetworkEvent<BlockEvent, Tip> {
    type From = super::Point;

    fn next_from(&self) -> Option<Self::From> {
        if let CardanoNetworkEvent::Block(block_event) = self {
            Some(super::Point::BlockHeader {
                slot_nb: block_event.slot_number,
                hash: block_event.id.clone(),
            })
        } else {
            None
        }
    }
}
