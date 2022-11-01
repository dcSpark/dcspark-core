pub mod cardano;
pub mod multiverse;
mod source;

pub use source::*;

pub trait GetNextFrom {
    type From: PullFrom + Clone;

    fn next_from(&self) -> Option<Self::From>;
}
