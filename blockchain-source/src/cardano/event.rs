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
    pub is_boundary_block: bool,
    pub epoch: Option<u64>,
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

                BlockEvent::from_serialized_block(raw_block).map_err(serde::de::Error::custom)
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

impl BlockEvent {
    pub(crate) fn from_serialized_block(raw_block: SerializedBlock) -> anyhow::Result<Self> {
        let block = raw_block
            .unserialize()
            .context("failed to deserialize block");

        if let Ok(block) = block {
            let id = BlockId::new(block.header().hash().to_string());
            let block_number = BlockNumber::new(block.header().block_number());

            let parent_id = get_parent_id(&block.header());

            Ok(BlockEvent {
                raw_block,
                id,
                parent_id,
                block_number,
                slot_number: SlotNumber::new(block.header().slot()),
                is_boundary_block: false,
                // this is not in the header, and computing it requires knowing the network
                // details, which makes implementing `Serialize` and `Deserialize`more complicated,
                // unless we serialize this field too.
                // it can be computed later inside carp, since we don't need this in the bridge.
                epoch: None,
            })
        } else if let Ok(block) = crate::cardano::byron::ByronBlock::decode(raw_block.as_ref()) {
            let header = block.header();
            let event = BlockEvent {
                raw_block,
                id: BlockId::new(block.hash().to_string()),
                parent_id: BlockId::new(header.previous_hash().to_string()),
                block_number: header.block_number(),
                slot_number: header.slot_number(),
                is_boundary_block: block.is_boundary(),
                epoch: Some(header.epoch()),
            };

            Ok(event)
        } else {
            tracing::error!(
                block = hex::encode(raw_block.as_ref()),
                "failed to deserialize block"
            );
            block.map(|_| unreachable! {})
        }
    }
}
