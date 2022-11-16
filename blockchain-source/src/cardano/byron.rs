//! The support in cardano-sdk for byron era block is a bit limited to the moment.
//!
//! This module fills some of those gaps. It should be removed once these these things are provided
//! there.
//!
//! Most notably
//!
//! - There is no support for boundary headers.
//! - The hashes for byron headers are computed incorrectly.
//!
//! This doesn't implement full support for the first point, we only parse what's needed to compute
//! the hash, parent, height, and epoch-slot of epoch boundary blocks.
use super::time::epoch_slot_to_absolute;
use anyhow::Context;
use cardano_sdk::chain::{
    byron::{self, ChainDifficulty},
    AnyCbor, HeaderHash,
};
use cbored::{CborDataOf, CborRepr};
use cryptoxide::hashing::blake2b_256;
use dcspark_core::{BlockNumber, SlotNumber};

#[derive(Debug, Clone, CborRepr, PartialEq, Eq)]
#[cborrepr(enumtype = "tagvariant")]
pub enum ByronBlock {
    ByronBoundary(Ebb),
    Byron(BlockByron),
}

#[derive(Clone, Debug, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct Ebb {
    pub header: CborDataOf<BoundaryHeader>,
    pub body: AnyCbor,
    pub extra: AnyCbor,
}

#[derive(Clone, Debug, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct BlockByron {
    // CborDataOf keeps the data unserialized, which is important for hashing because the current
    // representation of the byron::Header doesn't preserve the binary representation.
    pub header: CborDataOf<byron::Header>,
    pub body: AnyCbor,
    pub extra: AnyCbor,
}

#[derive(Debug, Clone, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct BoundaryHeader {
    pub protocol_magic: u64,
    pub previous_hash: HeaderHash,
    pub body_proof: AnyCbor,
    pub consensus: ConsensusBoundary,
    pub extra_data: AnyCbor,
}

#[derive(Debug, Clone, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct ConsensusBoundary {
    pub epoch: u64,
    pub chain_difficulty: ChainDifficulty,
}

pub enum ByronHeader {
    ByronBoundary(BoundaryHeader),
    Byron(Box<byron::Header>),
}

impl ByronBlock {
    pub(super) fn is_boundary(&self) -> bool {
        matches!(self, ByronBlock::ByronBoundary(_))
    }

    pub(super) fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        cbored::decode_from_bytes(bytes).context("couldn't decode byron block")
    }

    // cardano-sdk has a hash method for the byron header, but it's missing the prefix.
    //
    // it also doesn't support the epoch boundary header.
    pub(super) fn hash(&self) -> HeaderHash {
        let with_tag = match self {
            // ref: https://input-output-hk.github.io/ouroboros-network/cardano-ledger/src/Cardano.Chain.Block.Header.html#wrapBoundaryBytes
            ByronBlock::ByronBoundary(ebb) => [&[0x82, 0x00], ebb.header.as_ref()].concat(),
            // ref: https://input-output-hk.github.io/ouroboros-network/cardano-ledger/src/Cardano.Chain.Block.Header.html#wrapHeaderBytes
            ByronBlock::Byron(byron) => [&[0x82, 0x01], byron.header.as_ref()].concat(),
        };

        HeaderHash(blake2b_256(&with_tag))
    }

    pub(super) fn header(&self) -> ByronHeader {
        match self {
            ByronBlock::ByronBoundary(ebb) => ByronHeader::ByronBoundary(ebb.header.unserialize()),
            ByronBlock::Byron(byron) => ByronHeader::Byron(Box::new(byron.header.unserialize())),
        }
    }
}

impl ByronHeader {
    pub(super) fn previous_hash(&self) -> HeaderHash {
        match self {
            ByronHeader::ByronBoundary(header) => header.previous_hash.clone(),
            ByronHeader::Byron(header) => header.previous_hash.clone(),
        }
    }

    pub(super) fn block_number(&self) -> BlockNumber {
        BlockNumber::new(match self {
            ByronHeader::ByronBoundary(header) => header.consensus.chain_difficulty.0,
            ByronHeader::Byron(header) => header.consensus.chain_difficulty.0,
        })
    }

    pub(super) fn slot_number(&self) -> SlotNumber {
        SlotNumber::new(match self {
            ByronHeader::ByronBoundary(header) => epoch_slot_to_absolute(header.consensus.epoch, 0),
            ByronHeader::Byron(header) => {
                let slot = header.consensus.slot_id.slot_id.into();
                let epoch = header.consensus.slot_id.epoch;

                epoch_slot_to_absolute(epoch, slot)
            }
        })
    }

    pub(super) fn epoch(&self) -> u64 {
        match self {
            ByronHeader::ByronBoundary(header) => header.consensus.epoch,
            ByronHeader::Byron(header) => header.consensus.slot_id.epoch,
        }
    }
}
