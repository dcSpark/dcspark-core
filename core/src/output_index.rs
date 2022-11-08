use serde::{Deserialize, Serialize};
use std::{fmt, num, str};

/// use to identify a block number within the blockchain
///
/// this value is not necessarily monotonically increasing.
#[derive(
    Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct OutputIndex(u64);

impl OutputIndex {
    /// the largest value a [`OutputIndex`] can be
    pub const MAX: Self = Self::new(u64::MAX);

    /// the smallest value a [`OutputIndex`] can be.
    pub const MIN: Self = Self::new(u64::MIN);

    /// wrap the given value into a OutputIndex type
    ///
    #[inline(always)]
    pub const fn new(block_number: u64) -> Self {
        Self(block_number)
    }

    /// Try to increase by `1` the [`OutputIndex`]
    ///
    /// If the addition will overflow, the function will returns `None`.
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn checked_next(self) -> Option<Self> {
        self.checked_add(1)
    }

    /// Increase by `1` the [`OutputIndex`]
    ///
    /// If the addition will overflow, the function will returns [`Self::MAX`].
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn saturating_next(self) -> Self {
        self.saturating_add(1)
    }

    /// Try to add the right hand side (`rhs`) value to the [`OutputIndex`].
    ///
    /// If the addition will overflow, the function will returns `None`.
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn checked_add(self, rhs: u64) -> Option<Self> {
        self.0.checked_add(rhs).map(Self)
    }

    /// Add the right hand side (`rhs`) value to the [`OutputIndex`].
    ///
    /// If the addition will overflow we returns the [`Self::MAX`].
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }
}

impl fmt::Display for OutputIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Binary for OutputIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Octal for OutputIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::LowerHex for OutputIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperHex for OutputIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::LowerExp for OutputIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperExp for OutputIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl str::FromStr for OutputIndex {
    type Err = num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl From<u64> for OutputIndex {
    fn from(block_number: u64) -> Self {
        Self(block_number)
    }
}

impl From<OutputIndex> for u64 {
    fn from(OutputIndex(block_number): OutputIndex) -> Self {
        block_number
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smoke::{generator::num, property};
    use smoke_macros::smoketest;

    #[test]
    fn checked_next_overflow() {
        assert_eq!(None, OutputIndex::MAX.checked_next())
    }

    #[test]
    fn check_add_overflow() {
        assert_eq!(None, OutputIndex::MAX.checked_add(1))
    }

    #[test]
    fn saturating_next_overflow() {
        assert_eq!(OutputIndex::MAX, OutputIndex::MAX.saturating_next())
    }

    #[test]
    fn saturating_add_overflow() {
        assert_eq!(OutputIndex::MAX, OutputIndex::MAX.saturating_add(1))
    }

    #[smoketest{ a: num::<u64>() }]
    fn checked_next(a: u64) {
        property::equal(
            a.checked_add(1).map(OutputIndex),
            OutputIndex(a).checked_next(),
        )
    }

    #[smoketest{ a: num::<u64>(), b: num::<u64>() }]
    fn checked_add(a: u64, b: u64) {
        property::equal(
            a.checked_add(b).map(OutputIndex),
            OutputIndex(a).checked_add(b),
        )
    }

    #[smoketest{ a: num::<u64>() }]
    fn saturating_next(a: u64) {
        property::equal(
            OutputIndex(a.saturating_add(1)),
            OutputIndex(a).saturating_next(),
        )
    }

    #[smoketest{ a: num::<u64>(), b: num::<u64>() }]
    fn saturating_add(a: u64, b: u64) {
        property::equal(
            OutputIndex(a.saturating_add(b)),
            OutputIndex(a).saturating_add(b),
        )
    }
}
