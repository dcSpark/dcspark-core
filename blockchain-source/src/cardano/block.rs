use cardano_sdk::chain::{
    AnyCbor, Header, HeaderShelley, HeaderVasil, MetadataSet, TransactionBodies, TxIndexes,
};
use cbored::{CborRepr, DecodeError};

// Currently, cardano-sdk can't parse blocks that have Redeemer fields inside the witnesses,
// because of an issue deserializing the data field.
//
// Because for our use-cases these fields are not necessary anyway, we can just avoid the
// validation by parsing them as `AnyCbor`
//
// Unfortunately the only way of doing that easily (without patching the library) is to just
// re-define the block types here, this involves some code duplication, although it also gives us
// better control.

#[derive(Clone, Debug, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct BlockVasil {
    pub header: HeaderVasil,
    pub tx_bodies: TransactionBodies,
    pub tx_witnesses: AnyCbor,
    pub metadata_set: MetadataSet,
    pub invalid_tx: TxIndexes,
}

#[derive(Clone, Debug, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct BlockShelley {
    pub header: HeaderShelley,
    pub tx_bodies: TransactionBodies,
    pub tx_witnesses: AnyCbor,
    pub metadata_set: MetadataSet,
}

#[derive(Clone, Debug, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct BlockAlonzo {
    pub header: HeaderShelley,
    pub tx_bodies: TransactionBodies,
    pub tx_witnesses: AnyCbor,
    pub metadata_set: MetadataSet,
    pub invalid_tx: TxIndexes,
}

#[derive(Debug, Clone, CborRepr, PartialEq, Eq)]
#[cborrepr(enumtype = "tagvariant", variant_starts_at = 2)]
pub enum Block {
    Shelley(BlockShelley),
    Block3(BlockShelley),
    Block4(BlockShelley),
    Alonzo(BlockAlonzo),
    Vasil(BlockVasil),
}

impl Block {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        cbored::decode_from_bytes(bytes)
    }

    pub fn header(&self) -> Header {
        match self {
            Block::Shelley(blk) => blk.header.clone().into(),
            Block::Block3(blk) => blk.header.clone().into(),
            Block::Block4(blk) => blk.header.clone().into(),
            Block::Alonzo(blk) => blk.header.clone().into(),
            Block::Vasil(blk) => blk.header.clone().into(),
        }
    }

    pub fn tx_bodies(&self) -> &TransactionBodies {
        match self {
            Block::Shelley(blk) => &blk.tx_bodies,
            Block::Block3(blk) => &blk.tx_bodies,
            Block::Block4(blk) => &blk.tx_bodies,
            Block::Alonzo(blk) => &blk.tx_bodies,
            Block::Vasil(blk) => &blk.tx_bodies,
        }
    }
}
