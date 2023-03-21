use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt, str};

/// transaction identifier
///
/// needs to be in a human readable format. Usually this is going to be
/// in hexadecimal. However this is not necessarily guaranteed. Knowing
/// exactly the formatting is not necessary for what we intend to do any
/// way.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub struct TransactionId(Cow<'static, str>);

impl TransactionId {
    /// the transaction id that denote the absence of transaction identifier
    ///
    /// ```
    /// use dcspark_core::tx::TransactionId;
    ///
    /// assert_eq!(
    ///   TransactionId::ZERO,
    ///   TransactionId::new_static("N/A"),
    /// )
    /// ```
    pub const ZERO: Self = Self::new_static("N/A");

    pub fn new<B>(block_id: B) -> Self
    where
        B: Into<Cow<'static, str>>,
    {
        Self(block_id.into())
    }

    /// create a static [`TransactionId`]. Because we use a [`Cow`]
    /// internally this allows us to defined pre-defined static
    /// [`TransactionId`] without having to do extra allocations etc.
    pub const fn new_static(block_id: &'static str) -> Self {
        Self(Cow::Borrowed(block_id))
    }

    /// check the [`TransactionId`] starts with the given `prefix`.
    ///
    /// This can be useful to check for partial [`TransactionId`]
    pub fn starts_with<P>(&self, prefix: P) -> bool
    where
        P: AsRef<str>,
    {
        self.0.starts_with(prefix.as_ref())
    }
}

impl AsRef<str> for TransactionId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for TransactionId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with() {
        assert!(TransactionId::new_static("hello world").starts_with("hello"));
        assert!(!TransactionId::new_static("hello world").starts_with("world"));
    }
}
