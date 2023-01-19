use cardano_sdk::chain::{
    AnyCbor, BlockAlonzo, BlockShelley, Header, HeaderVasil, MetadataSet, TransactionBodies,
    TxIndexes,
};
use cbored::{CborRepr, DecodeError};

#[derive(Clone, Debug, CborRepr, PartialEq, Eq)]
#[cborrepr(structure = "array")]
pub struct BlockVasil {
    pub header: HeaderVasil,
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
