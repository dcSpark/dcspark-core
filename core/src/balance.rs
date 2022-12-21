use crate::Value;
use std::{
    any::type_name,
    fmt,
    ops::{Add, AddAssign, Sub, SubAssign},
};

pub enum Balance<Rep> {
    Debt(Value<Rep>),
    Balanced,
    Excess(Value<Rep>),
}

impl<Rep> Balance<Rep> {
    #[inline]
    pub fn zero() -> Self {
        Self::Balanced
    }

    #[inline]
    pub fn in_debt(&self) -> bool {
        matches!(self, Self::Debt(_))
    }

    #[inline]
    pub fn balanced(&self) -> bool {
        matches!(self, Self::Balanced)
    }

    #[inline]
    pub fn in_excess(&self) -> bool {
        matches!(self, Self::Excess(_))
    }
}

impl<Rep> Default for Balance<Rep> {
    fn default() -> Self {
        Self::Balanced
    }
}

impl<Rep> Clone for Balance<Rep> {
    fn clone(&self) -> Self {
        match self {
            Self::Debt(value) => Self::Debt(value.clone()),
            Self::Balanced => Self::Balanced,
            Self::Excess(value) => Self::Excess(value.clone()),
        }
    }
}

impl<Rep> PartialEq for Balance<Rep> {
    fn eq(&self, rhs: &Self) -> bool {
        match (self, rhs) {
            (Balance::Balanced, Balance::Balanced) => true,
            (Balance::Excess(lhs), Balance::Excess(rhs)) => lhs.eq(rhs),
            (Balance::Debt(lhs), Balance::Debt(rhs)) => lhs.eq(rhs),
            _ => false,
        }
    }
}

impl<Rep> Add for Balance<Rep> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        use std::cmp::Ordering::{Equal, Greater, Less};

        match (self, rhs) {
            (Self::Balanced, rhs) => rhs,
            (lhs, Self::Balanced) => lhs,
            (Self::Excess(lhs), Self::Excess(rhs)) => Self::Excess(lhs + rhs),
            (Self::Debt(lhs), Self::Debt(rhs)) => Self::Debt(lhs + rhs),
            (Self::Debt(lhs), Self::Excess(rhs)) => match lhs.cmp(&rhs) {
                Less => Self::Excess(rhs - lhs),
                Equal => Self::Balanced,
                Greater => Self::Debt(lhs - rhs),
            },
            (Self::Excess(lhs), Self::Debt(rhs)) => match lhs.cmp(&rhs) {
                Less => Self::Debt(rhs - lhs),
                Equal => Self::Balanced,
                Greater => Self::Excess(lhs - rhs),
            },
        }
    }
}

impl<'a, 'b, Rep> Add<&'b Value<Rep>> for &'a Balance<Rep> {
    type Output = Balance<Rep>;
    fn add(self, rhs: &'b Value<Rep>) -> Self::Output {
        use std::cmp::Ordering::{Equal, Greater, Less};

        match (self, rhs) {
            (Balance::Debt(lhs), rhs) => match lhs.cmp(rhs) {
                Less => Balance::Excess(rhs - lhs),
                Equal => Balance::Balanced,
                Greater => Balance::Debt(lhs - rhs),
            },
            (Balance::Balanced, rhs) => {
                if rhs == &Value::zero() {
                    Balance::Balanced
                } else {
                    Balance::Excess(rhs.clone())
                }
            }
            (Balance::Excess(lhs), rhs) => Balance::Excess(lhs + rhs),
        }
    }
}

impl<Rep> Add<Value<Rep>> for Balance<Rep> {
    type Output = Self;
    fn add(self, rhs: Value<Rep>) -> Self::Output {
        (&self).add(&rhs)
    }
}

impl<Rep> AddAssign<Value<Rep>> for Balance<Rep> {
    fn add_assign(&mut self, rhs: Value<Rep>) {
        self.add_assign(&rhs)
    }
}

impl<'a, Rep> AddAssign<&'a Value<Rep>> for Balance<Rep> {
    fn add_assign(&mut self, rhs: &'a Value<Rep>) {
        *self = (&*self) + rhs;
    }
}

impl<'a, 'b, Rep> Sub<&'b Value<Rep>> for &'a Balance<Rep> {
    type Output = Balance<Rep>;
    fn sub(self, rhs: &'b Value<Rep>) -> Self::Output {
        use std::cmp::Ordering::{Equal, Greater, Less};

        match (self, rhs) {
            (Balance::Debt(lhs), rhs) => Balance::Debt(lhs + rhs),
            (Balance::Balanced, rhs) => {
                if rhs == &Value::zero() {
                    Balance::Balanced
                } else {
                    Balance::Debt(rhs.clone())
                }
            }
            (Balance::Excess(lhs), rhs) => match lhs.cmp(rhs) {
                Less => Balance::Debt(rhs - lhs),
                Equal => Balance::Balanced,
                Greater => Balance::Excess(lhs - rhs),
            },
        }
    }
}

impl<Rep> Sub<Value<Rep>> for Balance<Rep> {
    type Output = Self;
    fn sub(self, rhs: Value<Rep>) -> Self::Output {
        (&self).sub(&rhs)
    }
}

impl<Rep> SubAssign<Value<Rep>> for Balance<Rep> {
    fn sub_assign(&mut self, rhs: Value<Rep>) {
        self.sub_assign(&rhs)
    }
}

impl<'a, Rep> SubAssign<&'a Value<Rep>> for Balance<Rep> {
    fn sub_assign(&mut self, rhs: &'a Value<Rep>) {
        *self = (&*self) - rhs;
    }
}

impl<Rep> fmt::Display for Balance<Rep> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Balanced => "0".fmt(f),
            Self::Debt(v) => write!(f, "-{v}"),
            Self::Excess(v) => write!(f, "+{v}"),
        }
    }
}

impl<Rep> fmt::Debug for Balance<Rep> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let rep = type_name::<Rep>();
        match self {
            Self::Balanced => f
                .debug_tuple(&format!("Balance::<{rep}>::Balanced"))
                .finish(),
            Self::Debt(v) => f
                .debug_tuple(&format!("Balance::<{rep}>::Debt"))
                .field(v)
                .finish(),
            Self::Excess(v) => f
                .debug_tuple(&format!("Balance::<{rep}>::Excess"))
                .field(v)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cardano;

    macro_rules! value {
        ($Value:literal) => {{
            Value::<cardano::Lovelace>::from($Value)
        }};
    }

    macro_rules! balance {
        (- $Value:literal) => {
            Balance::Debt(value!($Value))
        };
        (Balanced) => {
            Balance::Balanced
        };
        (+ $Value:literal) => {
            Balance::Excess(value!($Value))
        };
    }

    #[test]
    fn assign_sub() {
        let mut balance: Balance<cardano::Lovelace> = balance!(Balanced);
        balance -= value!(10);
        assert_eq!(balance, balance!(-10));

        let mut balance: Balance<cardano::Lovelace> = balance!(-10);
        balance -= value!(10);
        assert_eq!(balance, balance!(-20));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 5);
        balance -= value!(10);
        assert_eq!(balance, balance!(-5));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 10);
        balance -= value!(10);
        assert_eq!(balance, balance!(Balanced));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 10);
        balance -= value!(5);
        assert_eq!(balance, balance!(+ 5));
    }

    /// just like [`assign_sub`] but with subtracting references
    #[test]
    fn assign_sub_ref() {
        let mut balance: Balance<cardano::Lovelace> = balance!(Balanced);
        balance -= &value!(10);
        assert_eq!(balance, balance!(-10));

        let mut balance: Balance<cardano::Lovelace> = balance!(-10);
        balance -= &value!(10);
        assert_eq!(balance, balance!(-20));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 5);
        balance -= &value!(10);
        assert_eq!(balance, balance!(-5));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 10);
        balance -= &value!(10);
        assert_eq!(balance, balance!(Balanced));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 10);
        balance -= &value!(5);
        assert_eq!(balance, balance!(+ 5));
    }

    #[test]
    fn assign_add() {
        let mut balance: Balance<cardano::Lovelace> = balance!(Balanced);
        balance += value!(10);
        assert_eq!(balance, balance!(+10));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 10);
        balance += value!(10);
        assert_eq!(balance, balance!(+20));

        let mut balance: Balance<cardano::Lovelace> = balance!(-5);
        balance += value!(10);
        assert_eq!(balance, balance!(+5));

        let mut balance: Balance<cardano::Lovelace> = balance!(-10);
        balance += value!(10);
        assert_eq!(balance, balance!(Balanced));

        let mut balance: Balance<cardano::Lovelace> = balance!(-10);
        balance += value!(5);
        assert_eq!(balance, balance!(-5));
    }

    #[test]
    fn assign_add_ref() {
        let mut balance: Balance<cardano::Lovelace> = balance!(Balanced);
        balance += &value!(10);
        assert_eq!(balance, balance!(+10));

        let mut balance: Balance<cardano::Lovelace> = balance!(+ 10);
        balance += &value!(10);
        assert_eq!(balance, balance!(+20));

        let mut balance: Balance<cardano::Lovelace> = balance!(-5);
        balance += &value!(10);
        assert_eq!(balance, balance!(+5));

        let mut balance: Balance<cardano::Lovelace> = balance!(-10);
        balance += &value!(10);
        assert_eq!(balance, balance!(Balanced));

        let mut balance: Balance<cardano::Lovelace> = balance!(-10);
        balance += &value!(5);
        assert_eq!(balance, balance!(-5));
    }

    #[test]
    fn balance_add() {
        assert_eq!(balance!(Balanced) + value!(0), balance!(Balanced));
        assert_eq!(balance!(Balanced) + value!(1), balance!(+ 1));
        assert_eq!(balance!(+ 1) + value!(1), balance!(+ 2));
        assert_eq!(balance!(-1) + value!(1), balance!(Balanced));
        assert_eq!(balance!(-2) + value!(1), balance!(-1));
    }

    #[test]
    fn balance_sub() {
        assert_eq!(balance!(Balanced) - value!(0), balance!(Balanced));
        assert_eq!(balance!(Balanced) - value!(1), balance!(-1));
        assert_eq!(balance!(+ 1) - value!(1), balance!(Balanced));
        assert_eq!(balance!(+ 2) - value!(1), balance!(+ 1));
        assert_eq!(balance!(-1) - value!(1), balance!(-2));
    }
}
