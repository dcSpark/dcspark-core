use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt, ops::Deref, str};

/// on chain address
///
/// needs to be in a human readable format. Usually this is going to be
/// in hexadecimal. However this is not necessarily guaranteed. Knowing
/// exactly the formatting is not necessary for what we intend to do any
/// way.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub struct Address(Cow<'static, str>);

impl Address {
    pub fn new<B>(block_id: B) -> Self
    where
        B: Into<Cow<'static, str>>,
    {
        Self(block_id.into())
    }

    /// create a static [`Address`]. Because we use a [`Cow`]
    /// internally this allows us to defined pre-defined static
    /// [`Address`] without having to do extra allocations etc.
    pub const fn new_static(block_id: &'static str) -> Self {
        Self(Cow::Borrowed(block_id))
    }

    /// check the [`Address`] starts with the given `prefix`.
    ///
    /// This can be useful to check for partial [`Address`]
    pub fn starts_with<P>(&self, prefix: P) -> bool
    where
        P: AsRef<str>,
    {
        self.0.starts_with(prefix.as_ref())
    }
}

impl AsRef<str> for Address {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl Deref for Address {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl str::FromStr for Address {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with() {
        assert!(Address::new_static("hello world").starts_with("hello"));
        assert!(!Address::new_static("hello world").starts_with("world"));
    }
}
