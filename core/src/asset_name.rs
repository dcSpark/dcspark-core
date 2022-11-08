use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt};

/// identify an asset name through the protocol transfer
///
/// asset name is always represented as `[0; n]` encoded
/// in hexadecimal, n - is equal to the length of the set of bytes (there's no standard length)
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetName(Cow<'static, str>);

impl AssetName {
    /// default name of the main asset on cardano
    ///
    pub const MAIN: Self = Self(Cow::Borrowed("414441"));

    #[inline]
    pub fn new(asset_name: impl Into<Cow<'static, str>>) -> Self {
        Self(asset_name.into())
    }

    /// create a static [`AssetName`]. Because we use a [`Cow`]
    /// internally this allows us to defined pre-defined static
    /// [`AssetName`] without having to do extra allocations etc.
    pub const fn new_static(asset_name: &'static str) -> Self {
        Self(Cow::Borrowed(asset_name))
    }
}

impl AsRef<str> for AssetName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for AssetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
