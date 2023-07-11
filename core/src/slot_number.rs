use crate::NumberVisitor;
use serde::Serialize;
use std::{fmt, num, str};
/// use to identify a slot number within the blockchain
///
/// Not all blockchains have this kind of values. But if they
/// do have one it will be monotonically increasing.
#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Serialize)]
#[serde(transparent)]
pub struct SlotNumber(u64);

impl SlotNumber {
    /// the largest value a [`SlotNumber`] can be
    pub const MAX: Self = Self::new(u64::MAX);

    /// the smallest value a [`SlotNumber`] can be.
    pub const MIN: Self = Self::new(u64::MIN);

    /// wrap the given value into a SlotNumber type
    ///
    #[inline(always)]
    pub const fn new(block_number: u64) -> Self {
        Self(block_number)
    }

    /// Try to increase by `1` the [`SlotNumber`]
    ///
    /// If the addition will overflow, the function will returns `None`.
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn checked_next(self) -> Option<Self> {
        self.checked_add(1)
    }

    /// Increase by `1` the [`SlotNumber`]
    ///
    /// If the addition will overflow, the function will returns [`Self::MAX`].
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn saturating_next(self) -> Self {
        self.saturating_add(1)
    }

    /// Try to add the right hand side (`rhs`) value to the [`SlotNumber`].
    ///
    /// If the addition will overflow, the function will returns `None`.
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn checked_add(self, rhs: u64) -> Option<Self> {
        self.0.checked_add(rhs).map(Self)
    }

    /// Add the right hand side (`rhs`) value to the [`SlotNumber`].
    ///
    /// If the addition will overflow we returns the [`Self::MAX`].
    #[must_use = "The function does not modify the state, the new value is returned"]
    #[inline]
    pub fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }
}

impl fmt::Display for SlotNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Binary for SlotNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Octal for SlotNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::LowerHex for SlotNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperHex for SlotNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::LowerExp for SlotNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperExp for SlotNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl str::FromStr for SlotNumber {
    type Err = num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl From<u64> for SlotNumber {
    fn from(block_number: u64) -> Self {
        Self(block_number)
    }
}

impl From<SlotNumber> for u64 {
    fn from(SlotNumber(block_number): SlotNumber) -> Self {
        block_number
    }
}

/// Custom deserializer for SlotNumber(u64).
/// The deserialization is successful when the data (json) is a
/// number (u64) or when the data is a string (number base 10).
impl<'de> serde::de::Deserialize<'de> for SlotNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_any(NumberVisitor::<SlotNumber>::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use smoke::{generator::num, property};
    use smoke_macros::smoketest;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sample {
        n: u32,
        v: SlotNumber,
    }

    #[test]
    fn checked_next_overflow() {
        assert_eq!(None, SlotNumber::MAX.checked_next())
    }

    #[test]
    fn check_add_overflow() {
        assert_eq!(None, SlotNumber::MAX.checked_add(1))
    }

    #[test]
    fn saturating_next_overflow() {
        assert_eq!(SlotNumber::MAX, SlotNumber::MAX.saturating_next())
    }

    #[test]
    fn saturating_add_overflow() {
        assert_eq!(SlotNumber::MAX, SlotNumber::MAX.saturating_add(1))
    }

    #[smoketest{ a: num::<u64>() }]
    fn checked_next(a: u64) {
        property::equal(
            a.checked_add(1).map(SlotNumber),
            SlotNumber(a).checked_next(),
        )
    }

    #[smoketest{ a: num::<u64>(), b: num::<u64>() }]
    fn checked_add(a: u64, b: u64) {
        property::equal(
            a.checked_add(b).map(SlotNumber),
            SlotNumber(a).checked_add(b),
        )
    }

    #[smoketest{ a: num::<u64>() }]
    fn saturating_next(a: u64) {
        property::equal(
            SlotNumber(a.saturating_add(1)),
            SlotNumber(a).saturating_next(),
        )
    }

    #[smoketest{ a: num::<u64>(), b: num::<u64>() }]
    fn saturating_add(a: u64, b: u64) {
        property::equal(
            SlotNumber(a.saturating_add(b)),
            SlotNumber(a).saturating_add(b),
        )
    }

    #[test]
    fn deserialize_from_number() {
        let expected = Sample {
            n: 35,
            v: SlotNumber(1234),
        };

        let input = r#"
        {
            "n": 35,
            "v": 1234
        }
        "#;

        let output: Sample = serde_json::from_str(input).unwrap();
        assert_eq!(expected, output);
    }

    #[test]
    fn deserialize_from_string() {
        let expected = Sample {
            n: 70,
            v: SlotNumber(4567),
        };

        let input = r#"
        {
            "n": 70,
            "v": "4567"
        }
        "#;

        let output: Sample = serde_json::from_str(input).unwrap();
        assert_eq!(expected, output);
    }
}
