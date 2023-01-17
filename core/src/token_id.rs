use std::{borrow::Cow, fmt, str};

use serde::{Deserialize, Serialize};

/// identify a token through the protocol transfer
///
/// Token identifier is the unique representation of a specific token
/// for cardano it is the output of the CIP14 hashing, 0 padded.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenId(Cow<'static, str>);

impl TokenId {
    /// default value of the policyId
    ///
    pub const MAIN: Self = Self(Cow::Borrowed(
        "0000000000000000000000000000000000000000000000000000000000000000",
    ));

    #[inline]
    pub fn new(token_id: impl Into<Cow<'static, str>>) -> Self {
        Self(token_id.into())
    }

    /// create a static [`TokenId`]. Because we use a [`Cow`]
    /// internally this allows us to defined pre-defined static
    /// [`TokenId`] without having to do extra allocations etc.
    pub const fn new_static(token_id: &'static str) -> Self {
        Self(Cow::Borrowed(token_id))
    }
}

impl AsRef<str> for TokenId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

/// So, we don't have to change TokenId to `Option<TokenId>` in the code (except OutputTx where data arrives)
impl Default for TokenId {
    fn default() -> Self {
        TokenId::MAIN
    }
}

impl fmt::Display for TokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl str::FromStr for TokenId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.to_owned()))
    }
}
