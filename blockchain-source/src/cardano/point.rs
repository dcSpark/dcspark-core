use crate::PullFrom;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Point {
    Origin,
    BlockHeader { slot_nb: u64, hash: String },
}

impl PullFrom for Point {}
