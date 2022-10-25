use crate::NumberVisitor;
use serde::Serialize;
use std::{
    fmt, num, str,
    time::{Duration, SystemTime},
};

///
/// Use to define timestamp unix epoch
#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Serialize)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Timestamp {
    /// the largest value a [`Timestamp`] can be
    pub const MAX: Self = Self::new(u64::MAX);

    /// the smallest value a [`Timestamp`] can be.
    pub const MIN: Self = Self::new(u64::MIN);

    /// wrap the given value into a Timestamp type
    ///
    #[inline(always)]
    pub const fn new(timestamp: u64) -> Self {
        Self(timestamp)
    }

    #[inline(always)]
    pub fn into_inner(self) -> u64 {
        self.0
    }

    /// convert the given timestamp into a System UNIX time
    ///
    /// If you need to use a different referential use:
    ///
    /// ```no_compile
    /// SystemTime::BASE_TIME // your base time
    ///      + Timestamp(value).into_inner()
    /// ```
    #[inline]
    pub fn into_unix_time(self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(self.0)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl str::FromStr for Timestamp {
    type Err = num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl From<u64> for Timestamp {
    fn from(timestamp: u64) -> Self {
        Self(timestamp)
    }
}

impl From<Timestamp> for u64 {
    fn from(Timestamp(timestamp): Timestamp) -> Self {
        timestamp
    }
}

/// Custom deserializer for Timestamp(u64).
/// The deserialization is successful when the data (json) is a
/// number (u64) or when the data is a string (number base 10).
impl<'de> serde::de::Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_any(NumberVisitor::<Timestamp>::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deps::serde_json;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sample {
        n: u32,
        v: Timestamp,
    }

    #[test]
    fn deserialize_from_number() {
        let expected = Sample {
            n: 35,
            v: Timestamp(1650484805),
        };

        let input = r###"
        {
            "n": 35,
            "v": 1650484805
        }
        "###;

        let output: Sample = serde_json::from_str(input).unwrap();
        assert_eq!(expected, output);
    }

    #[test]
    fn deserialize_from_string() {
        let expected = Sample {
            n: 70,
            v: Timestamp(1650484805),
        };

        let input = r###"
        {
            "n": 70,
            "v": "1650484805"
        }
        "###;

        let output: Sample = serde_json::from_str(input).unwrap();
        assert_eq!(expected, output);
    }
}
