use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt, str};

/// block identifier
///
/// needs to be in a human readable format. Usually this is going to be
/// in hexadecimal. However this is not necessarily guaranteed. Knowing
/// exactly the formatting is not necessary for what we intend to do any
/// way.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub struct BlockId(Cow<'static, str>);

impl BlockId {
    pub fn new<B>(block_id: B) -> Self
    where
        B: Into<Cow<'static, str>>,
    {
        Self(block_id.into())
    }

    /// create a static [`BlockId`]. Because we use a [`Cow`]
    /// internally this allows us to defined pre-defined static
    /// [`BlockId`] without having to do extra allocations etc.
    pub const fn new_static(block_id: &'static str) -> Self {
        Self(Cow::Borrowed(block_id))
    }

    /// check the [`BlockId`] starts with the given `prefix`.
    ///
    /// This can be useful to check for partial [`BlockId`]
    pub fn starts_with<P>(&self, prefix: P) -> bool
    where
        P: AsRef<str>,
    {
        self.0.starts_with(prefix.as_ref())
    }
}

impl AsRef<str> for BlockId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<[u8]> for BlockId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref().as_bytes()
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with() {
        assert!(BlockId::new_static("hello world").starts_with("hello"));
        assert!(!BlockId::new_static("hello world").starts_with("world"));
    }
}
