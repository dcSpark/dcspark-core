use crate::cardano::time::Era;
use crate::{EventObject, GetNextFrom};
use anyhow::{anyhow};
use dcspark_core::{BlockId, BlockNumber, SlotNumber};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CardanoNetworkEvent<Block, Tip> {
    #[serde(skip)]
    Tip(Tip),
    Block(Block),
}

impl<Block: Send, Tip: Send> EventObject for CardanoNetworkEvent<Block, Tip> {
    fn is_blockchain_tip(&self) -> bool {
        matches!(self, CardanoNetworkEvent::Tip { .. })
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct BlockEvent {
    pub id: BlockId,
    pub parent_id: BlockId,
    pub block_number: BlockNumber,
    pub raw_block: Vec<u8>,
    pub slot_number: SlotNumber,
    pub is_boundary_block: bool,
    pub epoch: u64,
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

pub(crate) fn get_parent_id(header: &cml_multi_era::utils::MultiEraBlockHeader) -> BlockId {
    header
        .prev_hash()
        .as_ref()
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
    pub(crate) fn from_serialized_block(raw_block: &[u8], era: &Era) -> anyhow::Result<Self> {
        let block = cml_multi_era::MultiEraBlock::from_explicit_network_cbor_bytes(raw_block).expect("failed to deserialize block");
        let header = &block.header();
        Ok(BlockEvent {
            raw_block: raw_block.to_vec(),
            id: BlockId::new(hex::encode(block.hash())),
            parent_id: get_parent_id(header),
            block_number: BlockNumber::new(header.block_number()),
            slot_number: SlotNumber::new(header.slot()),
            is_boundary_block: match &block {
                cml_multi_era::MultiEraBlock::Byron(bb) => matches!(bb, cml_multi_era::byron::block::ByronBlock::EpochBoundary(_)),
                _ => false
            },
            // this is not in the header, and computing it requires knowing the network
            // details, which makes implementing `Serialize` and `Deserialize`more complicated,
            // unless we serialize this field too.
            // it can be computed later inside carp, since we don't need this in the bridge.
            epoch: match &block {
                cml_multi_era::MultiEraBlock::Byron(bb) => match bb {
                    cml_multi_era::byron::block::ByronBlock::EpochBoundary(eb) => eb.header.consensus_data.epoch_id,
                    cml_multi_era::byron::block::ByronBlock::Main(m) => m.header.consensus_data.byron_slot_id.epoch,
                },
                _ => era
                .absolute_slot_to_epoch(header.slot())
                .ok_or(anyhow!("can't detect epoch of block"))?
            },
        })
    }
}
