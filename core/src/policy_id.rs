use std::{borrow::Cow, fmt};

use serde::{Deserialize, Serialize};

/// identify a token through the protocol transfer
///
/// the token policy id is always represented as `[0; 56]` encoded
/// in hexadecimal
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyId(Cow<'static, str>);

impl PolicyId {
    #[inline]
    pub fn new(policy_id: impl Into<Cow<'static, str>>) -> Self {
        Self(policy_id.into())
    }

    /// create a static [`PolicyId`]. Because we use a [`Cow`]
    /// internally this allows us to defined pre-defined static
    /// [`PolicyId`] without having to do extra allocations etc.
    pub const fn new_static(token_id: &'static str) -> Self {
        Self(Cow::Borrowed(token_id))
    }
}

impl AsRef<str> for PolicyId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for PolicyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
