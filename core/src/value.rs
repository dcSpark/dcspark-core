use crate::BigDecimalVisitor;
use crate::TokenId;
use deps::bigdecimal::{
    num_bigint::BigInt, BigDecimal, FromPrimitive, One as _, Signed, ToPrimitive, Zero,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    hash::{Hash, Hasher},
    iter::Sum,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, Mul, Sub, SubAssign},
    str::{self, FromStr},
};
use thiserror::Error;

/**
Cardano value representation marker.
Value with this marker is stored in lovelace
 */
pub mod cardano {
    pub struct Ada;

    pub struct Lovelace;

    pub const ADA_LOVELACE_SCALE_FACTOR: i64 = 6;
}

/**
Evm sidechain value representation marker.
Value with this marker is stored in wei.
1 Ether = 1 ADA
 */
pub mod evm {
    pub struct Ether;

    pub struct Wei;

    pub const ETH_WEI_SCALE_FACTOR: i64 = 18;
}

/**
Algorand mainchain value representation marker.
Value with this marker is stored in wei.
1 Ether = 1 ALGO
 */
pub mod algo {
    pub struct Algo;

    pub struct MicroAlgo;

    pub const ALGO_MICRO_SCALE_FACTOR: i64 = 6;
}

/**
Normalized value representation marker.
Value with this marker is stored in ADA (not in lovelace).
 */
pub struct Normalized;

/**
A regulated value

This value is regulated and will need a special rule in order to be converted
from one side to the other
 */
pub struct Regulated;

/// a rule to convert value from the their respective representation
/// into a more appropriate value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub asset: TokenId,
    pub mainchain_decimal_precision: i64,
    pub sidechain_decimal_precision: i64,
}

/**
Value represents the amount of ada.
Rep parameter impacts scaling:
* Cardano representation is value in lovelace
* EVM representation is value in wei
* Normalized representation is value in ADA

1 ADA on mainchain matches to 1 Ether on sidechain

The purpose of this class is to remove hand-managed conversions between service e.g.
source, executor and so on.
 */
pub struct Value<Rep> {
    value: BigDecimal,
    phantom_data: PhantomData<fn() -> Rep>,
}

impl Value<Regulated> {
    pub fn normalize_from_mainchain(&self, rule: &Rule) -> Value<Normalized> {
        let value = scale(&self.value, rule.mainchain_decimal_precision);

        Value::new(value)
    }

    pub fn normalize_from_sidechain(&self, rule: &Rule) -> Value<Normalized> {
        let value = scale(&self.value, rule.sidechain_decimal_precision);

        Value::new(value)
    }

    pub fn from_normalized_to_mainchain(normalized: &Value<Normalized>, rule: &Rule) -> Self {
        let value = scale(&normalized.value, -rule.mainchain_decimal_precision);

        Value::new(value)
    }

    pub fn from_normalized_to_sidechain(normalized: &Value<Normalized>, rule: &Rule) -> Self {
        let value = scale(&normalized.value, -rule.sidechain_decimal_precision);

        Value::new(value)
    }
}

impl<Rep> Value<Rep> {
    /// coerce a value into a new representation
    ///
    /// This is unsafe so it's not wise to use in other place
    ///
    /// # Safety
    ///
    /// Using this function will affect the safe operation of Value
    /// and you may end up with invalid state.
    pub unsafe fn coerce<N>(self) -> Value<N> {
        Value::new(self.value)
    }

    pub fn new(value: BigDecimal) -> Self {
        Self {
            value,
            phantom_data: PhantomData,
        }
    }

    pub fn zero() -> Self {
        Self::new(BigDecimal::from(0u64))
    }

    pub fn one() -> Self {
        Self::new(BigDecimal::from(1u64))
    }

    pub fn raw(&self) -> &BigDecimal {
        &self.value
    }

    /// remove the decimals and keep only the integral part
    ///
    /// ```
    /// # use dcspark_core::{Value, Normalized};
    /// let value: Value<Normalized> = "1.029".parse().unwrap();
    /// let truncated = value.truncate();
    /// assert_eq!(truncated.to_string(), "1");
    /// ```
    ///
    pub fn truncate(&self) -> Self {
        let value = &self.value;

        let value = if value < &BigDecimal::one() {
            BigDecimal::from(0u64)
        } else {
            let value = value.to_string();
            let mut split = value.split('.');

            if let Some(integer) = split.next() {
                if let Ok(parsed) = integer.parse() {
                    parsed
                } else {
                    BigDecimal::zero()
                }
            } else {
                BigDecimal::zero()
            }
        };

        Self::new(value)
    }
}

impl<Rep> ToPrimitive for Value<Rep> {
    fn to_i64(&self) -> Option<i64> {
        self.value.to_i64()
    }

    fn to_u64(&self) -> Option<u64> {
        self.value.to_u64()
    }
}

impl Value<cardano::Ada> {
    /// normalize the Value from the Cardano Ada form to the [`Normalized`] form
    #[inline]
    pub fn normalize(&self) -> Value<Normalized> {
        Value {
            value: self.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn from_normalized(normalized: &Value<Normalized>) -> Self {
        Value {
            value: normalized.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn to_lovelace(&self) -> Value<cardano::Lovelace> {
        Value {
            value: scale(&self.value, -cardano::ADA_LOVELACE_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }
}

impl Value<cardano::Lovelace> {
    /// normalize the Value from the Cardano Lovelace form to the [`Normalized`] form
    #[inline]
    pub fn normalize(&self) -> Value<Normalized> {
        Value {
            value: scale(&self.value, cardano::ADA_LOVELACE_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn to_regulated(&self) -> Value<Regulated> {
        Value {
            value: self.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn from_regulated(regulated: &Value<Regulated>) -> Self {
        Value {
            value: regulated.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn from_normalized(normalized: &Value<Normalized>) -> Self {
        Value {
            value: scale(&normalized.value, -cardano::ADA_LOVELACE_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn to_ada(&self) -> Value<cardano::Ada> {
        Value {
            value: scale(&self.value, cardano::ADA_LOVELACE_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }
}

impl Value<evm::Ether> {
    /// normalize the Value from the EVM's Ether form to the [`Normalized`] form
    #[inline]
    pub fn normalize(&self) -> Value<Normalized> {
        Value {
            value: self.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn from_normalized(normalized: &Value<Normalized>) -> Self {
        Value {
            value: normalized.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn to_wei(&self) -> Value<evm::Wei> {
        Value {
            value: scale(&self.value, -evm::ETH_WEI_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }
}

impl Value<evm::Wei> {
    /// normalize the Value from the Cardano Lovelace form to the [`Normalized`] form
    #[inline]
    pub fn normalize(&self) -> Value<Normalized> {
        Value {
            value: scale(&self.value, evm::ETH_WEI_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn from_normalized(normalized: &Value<Normalized>) -> Self {
        Value {
            value: scale(&normalized.value, -evm::ETH_WEI_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn to_ether(&self) -> Value<evm::Ether> {
        Value {
            value: scale(&self.value, evm::ETH_WEI_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }
}

impl Value<algo::Algo> {
    /// normalize the Value from the Cardano Ada form to the [`Normalized`] form
    #[inline]
    pub fn normalize(&self) -> Value<Normalized> {
        Value {
            value: self.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn from_normalized(normalized: &Value<Normalized>) -> Self {
        Value {
            value: normalized.value.clone(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn to_microalgo(&self) -> Value<algo::MicroAlgo> {
        Value {
            value: scale(&self.value, -algo::ALGO_MICRO_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }
}

impl Value<algo::MicroAlgo> {
    /// normalize the Value from the Cardano Lovelace form to the [`Normalized`] form
    #[inline]
    pub fn normalize(&self) -> Value<Normalized> {
        Value {
            value: scale(&self.value, algo::ALGO_MICRO_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn from_normalized(normalized: &Value<Normalized>) -> Self {
        Value {
            value: scale(&normalized.value, -algo::ALGO_MICRO_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn to_algo(&self) -> Value<evm::Ether> {
        Value {
            value: scale(&self.value, algo::ALGO_MICRO_SCALE_FACTOR),
            phantom_data: PhantomData,
        }
    }
}

#[inline]
fn scale(value: &BigDecimal, scale: i64) -> BigDecimal {
    value * BigDecimal::new(BigInt::from(1u64), scale)
}

impl<Rep> Default for Value<Rep> {
    fn default() -> Self {
        Self::new(BigDecimal::default())
    }
}

impl From<Value<cardano::Lovelace>> for Value<Normalized> {
    fn from(lovelace: Value<cardano::Lovelace>) -> Self {
        lovelace.normalize()
    }
}

impl From<Value<cardano::Ada>> for Value<cardano::Lovelace> {
    fn from(ada: Value<cardano::Ada>) -> Self {
        ada.to_lovelace()
    }
}

impl From<Value<Normalized>> for Value<cardano::Lovelace> {
    fn from(normalized: Value<Normalized>) -> Self {
        Self::from_normalized(&normalized)
    }
}

impl From<Value<cardano::Ada>> for Value<Normalized> {
    fn from(cardano: Value<cardano::Ada>) -> Self {
        cardano.normalize()
    }
}

impl From<Value<Normalized>> for Value<cardano::Ada> {
    fn from(normalized: Value<Normalized>) -> Self {
        Self {
            value: normalized.value,
            phantom_data: PhantomData,
        }
    }
}

impl From<Value<evm::Wei>> for Value<Normalized> {
    fn from(evm: Value<evm::Wei>) -> Self {
        evm.normalize()
    }
}

impl From<Value<Normalized>> for Value<evm::Wei> {
    fn from(normalized: Value<Normalized>) -> Self {
        Self::from_normalized(&normalized)
    }
}

impl From<Value<evm::Ether>> for Value<Normalized> {
    fn from(evm: Value<evm::Ether>) -> Self {
        evm.normalize()
    }
}

impl From<Value<algo::MicroAlgo>> for Value<Normalized> {
    fn from(algo: Value<algo::MicroAlgo>) -> Self {
        algo.normalize()
    }
}

impl From<Value<Normalized>> for Value<algo::MicroAlgo> {
    fn from(normalized: Value<Normalized>) -> Self {
        Self::from_normalized(&normalized)
    }
}

impl From<Value<algo::Algo>> for Value<Normalized> {
    fn from(algorand: Value<algo::Algo>) -> Self {
        algorand.normalize()
    }
}

impl<Rep> From<BigDecimal> for Value<Rep> {
    fn from(value: BigDecimal) -> Self {
        Self::new(value)
    }
}

impl From<Value<Normalized>> for Value<evm::Ether> {
    fn from(normalized: Value<Normalized>) -> Self {
        Self {
            value: normalized.value,
            phantom_data: PhantomData,
        }
    }
}

impl<Rep> Clone for Value<Rep> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            phantom_data: self.phantom_data,
        }
    }
}

impl<Rep> From<u64> for Value<Rep> {
    fn from(value: u64) -> Self {
        Self {
            value: BigDecimal::from(value),
            phantom_data: PhantomData,
        }
    }
}

impl<Rep> fmt::Debug for Value<Rep> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple(&format!("Value<{}>", std::any::type_name::<Rep>()))
            .field(&self.value.to_string())
            .finish()
    }
}

impl<Rep> fmt::Display for Value<Rep> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[derive(Debug, Error)]
pub enum ValueFromStrError {
    #[error("Failed to parse big decimal: {0}")]
    InvalidDecimal(#[from] deps::bigdecimal::ParseBigDecimalError),

    #[error("Too many decimals: {current} is greater than {max}")]
    InvalidDecimalPoint { max: i64, current: i64 },

    #[error("Value cannot be negative")]
    Negative { value: BigDecimal },
}

macro_rules! derive_from_str {
    ($Type:ty, $MAX:expr) => {
        impl FromStr for Value<$Type> {
            type Err = ValueFromStrError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let value: BigDecimal = s.parse()?;
                let (_, current) = value.clone().into_bigint_and_exponent();
                if current > $MAX {
                    Err(ValueFromStrError::InvalidDecimalPoint { max: $MAX, current })
                } else if !value.is_positive() && !value.is_zero() {
                    Err(ValueFromStrError::Negative { value })
                } else {
                    Ok(Self {
                        value,
                        phantom_data: PhantomData,
                    })
                }
            }
        }
    };
}

derive_from_str!(Normalized, 18);
derive_from_str!(Regulated, 18);
derive_from_str!(cardano::Lovelace, 0);
derive_from_str!(cardano::Ada, cardano::ADA_LOVELACE_SCALE_FACTOR);
derive_from_str!(evm::Wei, 0);
derive_from_str!(evm::Ether, evm::ETH_WEI_SCALE_FACTOR);
derive_from_str!(algo::MicroAlgo, 0);
derive_from_str!(algo::Algo, algo::ALGO_MICRO_SCALE_FACTOR);

impl<Rep> PartialEq<Self> for Value<Rep> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

impl<Rep> Eq for Value<Rep> {}

impl<Rep> PartialOrd<Self> for Value<Rep> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<Rep> Ord for Value<Rep> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<Rep> Hash for Value<Rep> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.value.hash(hasher);
        self.phantom_data.hash(hasher);
    }
}

impl<'de, Rep> Deserialize<'de> for Value<Rep>
where
    Value<Rep>: FromStr,
    <Value<Rep> as FromStr>::Err: std::error::Error,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(BigDecimalVisitor::<Value<Rep>>::default())
    }
}

impl<Rep> Serialize for Value<Rep> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_newtype_struct("Value", &self.to_string())
    }
}

impl<Rep> Add for Value<Rep> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.value.add(rhs.value))
    }
}

impl<'a, Rep> Add<Value<Rep>> for &'a Value<Rep> {
    type Output = Value<Rep>;

    fn add(self, rhs: Value<Rep>) -> Self::Output {
        Value::new((&self.value).add(rhs.value))
    }
}

impl<'a, Rep> Add for &'a Value<Rep> {
    type Output = Value<Rep>;

    fn add(self, rhs: Self) -> Self::Output {
        Value::new((&self.value).add(&rhs.value))
    }
}

impl<'a, Rep> Add<&'a Value<Rep>> for Value<Rep> {
    type Output = Self;

    fn add(self, rhs: &'a Value<Rep>) -> Self::Output {
        Value::new(self.value.add(&rhs.value))
    }
}

impl<Rep> AddAssign for Value<Rep> {
    fn add_assign(&mut self, rhs: Self) {
        self.value.add_assign(rhs.value)
    }
}

impl<'a, Rep> AddAssign<&'a Value<Rep>> for Value<Rep> {
    fn add_assign(&mut self, rhs: &'a Value<Rep>) {
        self.value.add_assign(&rhs.value)
    }
}

impl<Rep> Sub for Value<Rep> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.value.sub(rhs.value))
    }
}

impl<'a, Rep> Sub<&'a Value<Rep>> for Value<Rep> {
    type Output = Self;
    fn sub(self, rhs: &'a Value<Rep>) -> Self::Output {
        Value::new(self.value.sub(&rhs.value))
    }
}

impl<'a, Rep> Sub for &'a Value<Rep> {
    type Output = Value<Rep>;

    fn sub(self, rhs: Self) -> Self::Output {
        Value::new((&self.value).sub(&rhs.value))
    }
}

impl<Rep> SubAssign for Value<Rep> {
    fn sub_assign(&mut self, rhs: Self) {
        self.value.sub_assign(rhs.value)
    }
}

impl<'a, Rep> SubAssign<&'a Value<Rep>> for Value<Rep> {
    fn sub_assign(&mut self, rhs: &'a Value<Rep>) {
        self.value.sub_assign(&rhs.value)
    }
}

impl<Rep> Div<usize> for Value<Rep> {
    type Output = Value<Rep>;

    fn div(self, rhs: usize) -> Self::Output {
        (&self).div(rhs)
    }
}

impl<'a, Rep> Div<usize> for &'a Value<Rep> {
    type Output = Value<Rep>;

    fn div(self, rhs: usize) -> Self::Output {
        let value = &self.value
            / BigDecimal::from_usize(rhs).expect("Usize should always fit in a big number");
        Value::new(value)
    }
}

impl<Rep> Sum for Value<Rep> {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self::new(iter.map(|v| v.value).sum())
    }
}

impl<'a, Rep> Sum<&'a Value<Rep>> for Value<Rep> {
    fn sum<I: Iterator<Item = &'a Value<Rep>>>(iter: I) -> Self {
        Self::new(iter.map(|v| &v.value).sum())
    }
}

macro_rules! mul_with {
    ($Type:ty, $Conv:expr) => {
        impl<Rep> Mul<$Type> for Value<Rep> {
            type Output = Self;
            fn mul(self, rhs: $Type) -> Self::Output {
                Self {
                    value: self.value.mul($Conv(rhs).unwrap()),
                    phantom_data: PhantomData,
                }
            }
        }

        impl<'a, Rep> Mul<$Type> for &'a Value<Rep> {
            type Output = Value<Rep>;
            fn mul(self, rhs: $Type) -> Self::Output {
                Value {
                    value: (&self.value).mul($Conv(rhs).unwrap()),
                    phantom_data: PhantomData,
                }
            }
        }
    };
}

mul_with!(u32, BigInt::from_u32);
mul_with!(usize, BigInt::from_usize);

impl Rule {
    pub const ERC20: Rule = Rule {
        mainchain_decimal_precision: 0,
        sidechain_decimal_precision: 18,
        asset: TokenId::new_static("All ERC20"),
    };
}

impl Default for Rule {
    fn default() -> Self {
        Self::ERC20
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deps::bigdecimal::{num_bigint::BigInt, BigDecimal, FromPrimitive};
    use quickcheck::{quickcheck, Arbitrary, Gen};

    impl Arbitrary for Value<cardano::Ada> {
        fn arbitrary(g: &mut Gen) -> Self {
            let value: Value<cardano::Lovelace> = Arbitrary::arbitrary(g);
            value.to_ada()
        }
    }

    impl Arbitrary for Value<cardano::Lovelace> {
        fn arbitrary(g: &mut Gen) -> Self {
            // on testnet and mainnet Lovelace goes from `0` to `45_000_000_000_000_000`
            const MAX: u64 = 45_000_000_000_000_000;
            let value = u64::arbitrary(g) % MAX;

            Value::new(
                BigDecimal::from_u64(value)
                    .expect("We should be able to represent a value from any u64 number"),
            )
        }
    }

    impl Arbitrary for Value<evm::Ether> {
        fn arbitrary(g: &mut Gen) -> Self {
            let value: Value<evm::Wei> = Arbitrary::arbitrary(g);
            value.to_ether()
        }
    }

    impl Arbitrary for Value<evm::Wei> {
        fn arbitrary(g: &mut Gen) -> Self {
            const MAX: u64 = u64::MAX;
            let value = u64::arbitrary(g) % MAX;

            Value::new(
                BigDecimal::from_u64(value)
                    .expect("We should be able to represent a value from any u64 number"),
            )
        }
    }

    impl Arbitrary for Value<Normalized> {
        fn arbitrary(g: &mut Gen) -> Self {
            Value::<cardano::Ada>::arbitrary(g).into()
        }
    }

    fn test_parse<Desc>(string: &str, expected: Value<Desc>)
    where
        Value<Desc>: FromStr,
        <Value<Desc> as FromStr>::Err: fmt::Debug,
    {
        let decoded: Value<Desc> = string.parse().unwrap();
        assert_eq!(
            decoded, expected,
            "Expected value {expected}, didn't match the value {decoded}"
        );
    }

    #[test]
    fn parse_lovelace() {
        test_parse::<cardano::Lovelace>("0", Value::from(0u64));
        test_parse::<cardano::Lovelace>("42", Value::from(42u64));
        test_parse::<cardano::Lovelace>("1000000", Value::from(1_000_000u64));
        test_parse::<cardano::Lovelace>(
            "45000000000000000",
            Value::from(45_000_000_000_000_000u64),
        );
    }

    #[test]
    fn parse_ada() {
        test_parse::<cardano::Ada>("0", Value::from(0u64));
        test_parse::<cardano::Ada>("0.000001", Value::<cardano::Lovelace>::from(1u64).to_ada());
        test_parse::<cardano::Ada>("1", Value::<cardano::Lovelace>::from(1_000_000u64).to_ada());
    }

    #[test]
    fn parse_regulated_to_normalized() {
        let rule = Rule {
            asset: TokenId::new("Test"),
            mainchain_decimal_precision: 6,
            sidechain_decimal_precision: 6,
        };
        let regulated_value: Value<Regulated> = Value::from(100);
        let normalized = regulated_value.normalize_from_mainchain(&rule);

        let expected_value: Value<Normalized> = "0.0001".parse().unwrap();

        assert_eq!(normalized, expected_value);
    }

    #[test]
    fn truncate() {
        fn test_truncate<Desc>(string: &str, expected: Value<Desc>)
        where
            Value<Desc>: FromStr,
            <Value<Desc> as FromStr>::Err: fmt::Debug,
        {
            let value: Value<Desc> = string.parse().unwrap();
            assert_eq!(value.truncate(), expected, "failed to truncate {string}");
        }

        test_truncate::<Normalized>("0", Value::zero());
        test_truncate::<Normalized>("0.1", Value::zero());
        test_truncate::<Normalized>("0.9", Value::zero());
        test_truncate::<Normalized>("0.01", Value::zero());
        test_truncate::<Normalized>("0.09", Value::zero());

        test_truncate::<Normalized>("1", Value::from(1));
        test_truncate::<Normalized>("1.1", Value::from(1));
        test_truncate::<Normalized>("1.9", Value::from(1));
        test_truncate::<Normalized>("1.01", Value::from(1));
        test_truncate::<Normalized>("1.09", Value::from(1));

        test_truncate::<Normalized>("2", Value::from(2));
        test_truncate::<Normalized>("2.1", Value::from(2));
        test_truncate::<Normalized>("2.9", Value::from(2));
        test_truncate::<Normalized>("2.01", Value::from(2));
        test_truncate::<Normalized>("2.09", Value::from(2));

        test_truncate::<Normalized>("3.000000", Value::from(3));

        let value: Value<evm::Wei> = "3000000000000000000".parse().unwrap();
        let value: Value<Normalized> = value.normalize();
        let value = Value::<cardano::Lovelace>::from_normalized(&value);
        let value = value.truncate();
        assert_eq!(
            value.to_string(),
            Value::<cardano::Lovelace>::from(3000000).to_string()
        );
    }

    #[test]
    #[should_panic]
    fn ada_from_str_too_many_decimals() {
        let _value: Value<cardano::Ada> = "0.0000001".parse().unwrap();
    }
    #[test]
    #[should_panic]
    fn lovelace_from_str_too_many_decimals() {
        let _value: Value<cardano::Lovelace> = "0.1".parse().unwrap();
    }
    #[test]
    #[should_panic]
    fn wei_from_str_too_many_decimals() {
        let _value: Value<evm::Wei> = "0.1".parse().unwrap();
    }
    #[test]
    #[should_panic]
    fn ether_from_str_too_many_decimals() {
        let _value: Value<evm::Ether> = "0.0000000000000000001".parse().unwrap();
    }

    #[test]
    fn lovelace_to_normalized_manual() {
        // in lovelace
        let cardano_value: Value<cardano::Lovelace> = Value::new(BigDecimal::from(1u64));
        let normalized: Value<Normalized> = Value::from(cardano_value);
        assert_eq!("0.000001", normalized.to_string())
    }

    #[test]
    fn wei_to_normalized_manual() {
        // in wei
        let evm_value: Value<evm::Wei> = Value::new(BigDecimal::new(BigInt::from(1u64), -12));
        let normalized: Value<Normalized> = Value::from(evm_value);
        assert_eq!("0.000001", normalized.raw().to_string())
    }

    quickcheck! {
        fn lovelace_to_normalized_and_back(lovelace: Value<cardano::Lovelace>) -> bool {
            let normalized: Value<Normalized> = Value::from(lovelace.clone());
            let retrieved: Value<cardano::Lovelace> = Value::from(normalized);
            lovelace == retrieved
        }

        fn normalized_to_wei_back(normalized: Value<Normalized>) -> bool {
            let wei: Value<evm::Wei> = Value::from(normalized.clone());
            let normalized_back: Value<Normalized> = Value::from(wei);
            normalized == normalized_back
        }

        fn normalized_to_cardano_back(normalized: Value<Normalized>) -> bool {
            let lovelace: Value<cardano::Lovelace> = Value::from(normalized.clone());
            let normalized_back: Value<Normalized> = Value::from(lovelace);
            normalized == normalized_back
        }

        fn wei_to_string_from_str(ether: Value<evm::Wei>) -> bool {
            let s = ether.to_string();
            let value: Value<evm::Wei> = s.parse().unwrap();
            ether == value
        }

        fn ether_to_string_from_str(ether: Value<evm::Ether>) -> bool {
            let s = ether.to_string();
            let value: Value<evm::Ether> = s.parse().unwrap();
            ether == value
        }

        fn ada_to_string_from_str(ether: Value<cardano::Ada>) -> bool {
            let s = ether.to_string();
            let value: Value<cardano::Ada> = s.parse().unwrap();
            ether == value
        }

        fn lovelace_to_string_from_str(ether: Value<cardano::Lovelace>) -> bool {
            let s = ether.to_string();
            let value: Value<cardano::Lovelace> = s.parse().unwrap();
            ether == value
        }
    }

    macro_rules! value {
        ($Value:literal) => {{
            Value::<cardano::Lovelace>::from($Value)
        }};
    }

    #[test]
    fn math() {
        assert_eq!(value!(1) + value!(1) - value!(1), value!(1));
        assert_eq!(
            value!(10) - value!(1) - value!(2) + value!(1) - value!(1),
            value!(7)
        );
    }

    #[test]
    fn add() {
        assert_eq!(value!(1), value!(1));
        assert_eq!(value!(0) + value!(1), value!(1));
        assert_eq!(value!(1) + value!(1), value!(2));
        assert_eq!(value!(2) + value!(1), value!(3));
        assert_eq!(value!(40) + value!(2), value!(42));
        assert_eq!(value!(1) + value!(1) + value!(1), value!(3));
        assert_eq!(value!(1) + value!(2) + value!(1), value!(4));
        assert_eq!(
            value!(1) + value!(1) + value!(1) + value!(1) + value!(1),
            value!(5)
        );
    }

    #[test]
    fn add_ref() {
        assert_eq!(value!(0) + &value!(1), value!(1));
        assert_eq!(value!(1) + &value!(1), value!(2));
        assert_eq!(value!(2) + &value!(1), value!(3));
        assert_eq!(value!(40) + &value!(2), value!(42));
        assert_eq!(value!(1) + &value!(1) + &value!(1), value!(3));
        assert_eq!(value!(1) + &value!(2) + &value!(1), value!(4));
        assert_eq!(
            value!(1) + &value!(1) + &value!(1) + &value!(1) + &value!(1),
            value!(5)
        );
    }

    #[test]
    fn sub() {
        assert_eq!(value!(1) - value!(1), value!(0));
        assert_eq!(value!(2) - value!(1), value!(1));
        assert_eq!(value!(44) - value!(2), value!(42));
        assert_eq!(value!(10) - value!(1) - value!(1) - value!(1), value!(7));
        assert_eq!(value!(10) - value!(1) - value!(1) - value!(1), value!(7));
        assert_eq!(value!(10) - value!(1) - value!(2) - value!(1), value!(6));
        assert_eq!(
            value!(10) - value!(1) - value!(1) - value!(1) - value!(1) - value!(1),
            value!(5)
        );
    }

    #[test]
    fn sub_ref() {
        assert_eq!(value!(1) - &value!(1), value!(0));
        assert_eq!(value!(2) - &value!(1), value!(1));
        assert_eq!(value!(44) - &value!(2), value!(42));
        assert_eq!(value!(10) - &value!(1) - &value!(1) - &value!(1), value!(7));
        assert_eq!(value!(10) - &value!(1) - &value!(1) - &value!(1), value!(7));
        assert_eq!(value!(10) - &value!(1) - &value!(2) - &value!(1), value!(6));
        assert_eq!(
            value!(10) - &value!(1) - &value!(1) - &value!(1) - &value!(1) - &value!(1),
            value!(5)
        );
    }

    #[test]
    fn div() {
        assert_eq!(value!(1) / 1, value!(1));
        assert_eq!(value!(2) / 2, value!(1));
        assert_eq!((value!(3) / 2).truncate(), value!(1));

        assert_eq!((value!(1) / 2).truncate(), value!(0));
        assert_eq!((value!(4) / 3).truncate(), value!(1));
    }

    #[test]
    fn normalized() {
        const RULE: Rule = Rule {
            asset: TokenId::new_static("asset"),
            mainchain_decimal_precision: 6,
            sidechain_decimal_precision: 6,
        };

        let cardano = Value::<Regulated>::from(10_000_000);
        let sidechain = Value::<Regulated>::from(10_000_000);
        let intermediate = Value::<Normalized>::from(10);

        let value = cardano.normalize_from_mainchain(&RULE);

        assert_eq!(intermediate, value);

        let value = Value::<Regulated>::from_normalized_to_sidechain(&value, &RULE);

        assert_eq!(sidechain, value);
    }
}
