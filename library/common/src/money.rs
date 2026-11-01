//! Exact, unbounded monetary amounts.
//!
//! Every other module in the workspace works in `f64`, which is fine for
//! parity with the Python services that produced them but caps out around
//! 15 significant digits: past that, cents vanish, and sums of ordinary
//! large balances drift. `Money` is the type to use wherever an amount can
//! be arbitrarily large: it is a decimal with unbounded precision and
//! scale, so a billion-trillion-unit balance, a hyperinflated currency or a
//! ledger of millions of entries all stay exact.
//!
//! The type serialises as a JSON *string* so that no amount is ever
//! round-tripped through a binary float.
use std::cmp::Ordering;
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::str::FromStr;

use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Anything that can go wrong converting into or out of a [`Money`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoneyError(pub String);

impl fmt::Display for MoneyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MoneyError {}

/// An exact decimal amount with unbounded magnitude and precision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money(BigDecimal);

impl Money {
    /// An exact zero.
    pub fn zero() -> Self {
        Money(zero_dec())
    }

    /// Build from a decimal literal. No precision is lost and no exponent is
    /// rejected, so `"1e400"` and a 400-digit integer are both fine.
    pub fn parse(text: &str) -> Result<Self, MoneyError> {
        BigDecimal::from_str(text.trim())
            .map(Money)
            .map_err(|e| MoneyError(format!("not a valid amount: {text} ({e})")))
    }

    /// Lossless conversion from an integer amount.
    pub fn from_i64(value: i64) -> Self {
        Money(BigDecimal::from(value))
    }

    /// Lossless conversion from a `u64`.
    pub fn from_u64(value: u64) -> Self {
        Money(BigDecimal::from(value))
    }

    /// Convert a float through its shortest round-trip decimal form. The
    /// float is *not* the source of truth for money, but this keeps a
    /// `0.1` coming in from an API as exactly `0.1` rather than
    /// `0.1000000000000000055511151231257827`.
    pub fn from_f64_shortest(value: f64) -> Result<Self, MoneyError> {
        if !value.is_finite() {
            return Err(MoneyError(format!("amount must be finite, got {value}")));
        }
        Money::parse(&format!("{value}"))
    }

    /// The underlying decimal, for callers that need bigdecimal directly.
    pub fn as_decimal(&self) -> &BigDecimal {
        &self.0
    }

    /// Escape hatch for reporting code that still expects a float. This is
    /// lossy above ~15 significant digits by definition of the type.
    pub fn to_f64_lossy(&self) -> f64 {
        self.0.to_f64().unwrap_or(f64::NAN)
    }

    /// Render with a fixed number of decimals, rounding half to even. Unlike
    /// the decimal's own `Display`, trailing zeros are kept, so a report
    /// column never mixes "0" with "0.00".
    pub fn to_fixed(&self, scale: i64) -> String {
        format!("{:.*}", scale.max(0) as usize, self.round(scale).0)
    }

    /// Round to `scale` decimals using banker's rounding, matching the
    /// `py_round` helper used everywhere else in the workspace.
    pub fn round(&self, scale: i64) -> Self {
        if scale <= 0 {
            return Money(self.0.with_scale_round(0, RoundingMode::HalfEven));
        }
        Money(self.0.with_scale_round(scale, RoundingMode::HalfEven))
    }

    /// Absolute value.
    pub fn abs(&self) -> Self {
        Money(self.0.abs())
    }

    /// Negation helper for readable call sites.
    pub fn negate(&self) -> Self {
        Money(-self.0.clone())
    }

    /// True when the amount is exactly zero.
    pub fn is_zero(&self) -> bool {
        self.0 == zero_dec()
    }

    /// True when the amount is strictly below zero.
    pub fn is_negative(&self) -> bool {
        self.0 < zero_dec()
    }

    /// Multiply by a rate expressed as a fraction (`0.075` = 7.5%) and round
    /// the product to `scale` decimals.
    pub fn percentage(&self, rate: &Money, scale: i64) -> Self {
        self.times(rate).round(scale)
    }

    /// Effective tax/interest rate in percent, rounded, with the zero-guard
    /// the Python services use (a zero denominator yields zero, not an
    /// error and not infinity).
    pub fn ratio_pct(&self, denominator: &Money, scale: i64) -> Self {
        if denominator.is_zero() {
            Money::zero().round(scale)
        } else {
            (self.over(denominator) * Money::from_i64(100)).round(scale)
        }
    }

    /// Exact addition.
    pub fn plus(&self, other: &Money) -> Self {
        Money(&self.0 + &other.0)
    }

    /// Exact subtraction.
    pub fn minus(&self, other: &Money) -> Self {
        Money(&self.0 - &other.0)
    }

    /// Exact multiplication. The scale is the sum of the operands' scales, so
    /// no digit is discarded; round afterwards when a report scale is needed.
    pub fn times(&self, other: &Money) -> Self {
        Money(&self.0 * &other.0)
    }

    /// Division carried to 34 significant decimals before any rounding, so a
    /// third of a large principal keeps every cent that exists.
    pub fn over(&self, other: &Money) -> Self {
        if other.is_zero() {
            return Money::zero();
        }
        Money((&self.0 / &other.0).with_scale_round(34, RoundingMode::HalfEven))
    }

    /// Sum a collection exactly, without the accumulation error a float fold
    /// introduces on long ledgers.
    pub fn sum<'a, I: IntoIterator<Item = &'a Money>>(items: I) -> Self {
        items.into_iter().fold(Money::zero(), |acc, m| acc.plus(m))
    }

    /// Split an amount across weights so that the parts add back to the
    /// total to the last minor unit. Undistributed minor units go to the
    /// largest fractional remainders, ties broken by position, which is the
    /// behaviour the accounting services want and float arithmetic cannot
    /// guarantee. Negative weights are treated as zero.
    pub fn allocate(&self, weights: &[Money], scale: i64) -> Vec<Money> {
        if weights.is_empty() {
            return Vec::new();
        }
        let factor = pow10(scale);
        let target_minor = self.round(scale).times(&factor).round(0);
        let effective: Vec<Money> = weights
            .iter()
            .map(|w| {
                if w.is_negative() {
                    Money::zero()
                } else {
                    w.clone()
                }
            })
            .collect();
        let weight_total = Money::sum(&effective);

        if weight_total.is_zero() {
            return split_evenly(&target_minor, weights.len(), scale);
        }

        let mut floors = Vec::with_capacity(effective.len());
        let mut remainders = Vec::with_capacity(effective.len());
        for (i, weight) in effective.iter().enumerate() {
            let share = target_minor.times(weight).over(&weight_total);
            let floor = share.round(0);
            remainders.push((i, share.minus(&floor)));
            floors.push(floor);
        }

        let mut allocated = Money::sum(&floors);
        let mut leftover = target_minor.minus(&allocated);
        remainders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut idx = 0;
        while !leftover.is_zero() {
            let (target, _) = remainders[idx % remainders.len()];
            floors[target] = floors[target].plus(&Money::from_i64(1));
            leftover = leftover.minus(&Money::from_i64(1));
            allocated = allocated.plus(&Money::from_i64(1));
            idx += 1;
        }
        let _ = allocated;
        floors
            .into_iter()
            .map(|m| m.over(&factor).round(scale))
            .collect()
    }
}

/// An exact BigDecimal zero.
fn zero_dec() -> BigDecimal {
    BigDecimal::from(0)
}

/// 10^scale as an exact decimal.
fn pow10(scale: i64) -> Money {
    let mut acc = Money::from_i64(1);
    for _ in 0..scale.max(0) {
        acc = acc.times(&Money::from_i64(10));
    }
    acc
}

/// Distribute an integer amount of minor units evenly, giving the spare units
/// to the earliest positions.
fn split_evenly(total_minor: &Money, parts: usize, scale: i64) -> Vec<Money> {
    let factor = pow10(scale);
    let n = Money::from_i64(parts as i64);
    let base = total_minor.over(&n).round(0);
    let mut result: Vec<Money> = vec![base.clone(); parts];
    let covered = base.times(&n);
    let mut spare = total_minor.minus(&covered).round(0);
    let mut i = 0;
    while !spare.is_zero() && i < parts {
        result[i] = result[i].plus(&Money::from_i64(1));
        spare = spare.minus(&Money::from_i64(1));
        i += 1;
    }
    result
        .into_iter()
        .map(|m| m.over(&factor).round(scale))
        .collect()
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Money {
    type Err = MoneyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Money::parse(s)
    }
}

impl Serialize for Money {
    /// Always a string, never a JSON number: the whole point of the type is
    /// that no consumer re-parses it as a binary float.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(deserializer)?;
        match &raw {
            serde_json::Value::String(s) => Money::parse(s).map_err(serde::de::Error::custom),
            serde_json::Value::Number(n) => {
                Money::parse(&n.to_string()).map_err(serde::de::Error::custom)
            }
            other => Err(serde::de::Error::custom(format!(
                "amount must be a string or number, got {other}"
            ))),
        }
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(&self.0 + &rhs.0)
    }
}

impl<'a> Add<&'a Money> for &'a Money {
    type Output = Money;
    fn add(self, rhs: &'a Money) -> Money {
        Money(&self.0 + &rhs.0)
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(&self.0 - &rhs.0)
    }
}

impl<'a> Sub<&'a Money> for &'a Money {
    type Output = Money;
    fn sub(self, rhs: &'a Money) -> Money {
        Money(&self.0 - &rhs.0)
    }
}

impl Mul for Money {
    type Output = Money;
    fn mul(self, rhs: Money) -> Money {
        Money(&self.0 * &rhs.0)
    }
}

impl<'a> Mul<&'a Money> for &'a Money {
    type Output = Money;
    fn mul(self, rhs: &'a Money) -> Money {
        Money(&self.0 * &rhs.0)
    }
}

impl Div for Money {
    type Output = Money;
    fn div(self, rhs: Money) -> Money {
        Money(&self.0 / &rhs.0)
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money::zero(), |acc, m| acc.plus(&m))
    }
}

impl PartialEq<BigDecimal> for Money {
    fn eq(&self, other: &BigDecimal) -> bool {
        &self.0 == other
    }
}

impl PartialOrd<BigDecimal> for Money {
    fn partial_cmp(&self, other: &BigDecimal) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("valid amount")
    }

    #[test]
    fn keeps_every_digit_of_an_astronomical_amount() {
        // a 60-digit figure, then the same amount with a single unit added:
        // the difference must survive exactly, which no f64 can do
        let huge = m("123456789012345678901234567890123456789012345678901234567890");
        let plus_one = m("123456789012345678901234567890123456789012345678901234567891");
        assert_eq!(plus_one.minus(&huge), Money::from_i64(1));
        // the same two amounts are identical once forced through a float
        assert_eq!(huge.to_f64_lossy(), plus_one.to_f64_lossy());
    }

    #[test]
    fn survives_hyperinflation_and_scientific_notation() {
        let note = m("1e400");
        let other = m("3e399");
        assert_eq!(note.minus(&other), m("7e399"));
        assert!(note.to_f64_lossy().is_infinite());
        // and a 400-digit integer parses without complaint
        let long = "9".repeat(400);
        assert_eq!(m(&long).to_string(), long);
    }

    #[test]
    fn sums_a_ledger_without_float_drift() {
        // 0.1 as a float, ten million times, is famously not 1,000,000
        let tenth = m("0.1");
        let total = Money::sum(std::iter::repeat_n(&tenth, 1_000_000));
        assert_eq!(total.to_fixed(1), "100000.0");
        // the float route drifts
        let float_total: f64 = std::iter::repeat_n(0.1_f64, 1_000_000).sum();
        assert!((float_total - 100_000.0).abs() > 1e-6);
    }

    #[test]
    fn rounds_half_to_even_like_python() {
        assert_eq!(m("2.675").round(2).to_fixed(2), "2.68");
        assert_eq!(m("0.125").round(2).to_fixed(2), "0.12");
        assert_eq!(m("2.5").round(0).to_fixed(0), "2");
        assert_eq!(m("3.5").round(0).to_fixed(0), "4");
        assert_eq!(m("-2.675").round(2).to_fixed(2), "-2.68");
    }

    #[test]
    fn allocation_never_loses_a_minor_unit() {
        let total = m("100.00");
        let weights = vec![m("1"), m("1"), m("1")];
        let parts = total.allocate(&weights, 2);
        assert_eq!(parts.len(), 3);
        assert_eq!(Money::sum(&parts).to_fixed(2), "100.00");
        // equal remainders resolve by position, so the spare unit lands on
        // the first part
        assert_eq!(parts[0].to_fixed(2), "33.34");
        assert_eq!(parts[1].to_fixed(2), "33.33");
        assert_eq!(parts[2].to_fixed(2), "33.33");
    }

    #[test]
    fn allocation_handles_large_and_uneven_weights() {
        let total = m("123456789012345678901.55");
        let weights = vec![m("0.6"), m("0.25"), m("0.15")];
        let parts = total.allocate(&weights, 2);
        assert_eq!(Money::sum(&parts).to_fixed(2), "123456789012345678901.55");
        assert_eq!(parts[0].to_fixed(2), "74074073407407407340.93");
        assert_eq!(parts[1].to_fixed(2), "30864197253086419725.39");
        assert_eq!(parts[2].to_fixed(2), "18518518351851851835.23");
    }

    #[test]
    fn allocation_with_no_weights_splits_evenly() {
        let parts = m("10").allocate(&[Money::zero(), Money::zero()], 2);
        assert_eq!(parts[0].to_fixed(2), "5.00");
        assert_eq!(parts[1].to_fixed(2), "5.00");
        let odd = m("100.01").allocate(&[Money::zero(), Money::zero()], 2);
        assert_eq!(Money::sum(&odd).to_fixed(2), "100.01");
        assert_eq!(odd[0].to_fixed(2), "50.01");
        assert_eq!(odd[1].to_fixed(2), "50.00");
    }

    #[test]
    fn negative_weights_are_treated_as_zero() {
        let parts = m("50").allocate(&[m("-5"), m("5")], 2);
        assert_eq!(parts[0].to_fixed(2), "0.00");
        assert_eq!(parts[1].to_fixed(2), "50.00");
    }

    #[test]
    fn percentages_and_python_zero_guards() {
        let principal = m("1000000000000000000.00");
        let rate = m("0.075");
        assert_eq!(
            principal.percentage(&rate, 2).to_fixed(2),
            "75000000000000000.00"
        );
        assert_eq!(m("5").ratio_pct(&Money::zero(), 4).to_fixed(4), "0.0000");
        assert_eq!(m("5").ratio_pct(&m("8"), 4).to_fixed(4), "62.5000");
    }

    #[test]
    fn division_keeps_cents_on_large_principals() {
        let principal = m("999999999999999999999.99");
        let third = principal.over(&Money::from_i64(3));
        let back = third.times(&Money::from_i64(3));
        // within one minor unit of the principal, which is all a third can be
        let drift = back.minus(&principal).abs();
        // a third of 21 digits of cents cannot come back exactly, so allow a
        // single minor unit of drift and no more
        assert!(drift <= m("0.01"));
        assert!(drift <= m("1"));
    }

    #[test]
    fn serde_round_trips_exactly_as_a_string() {
        let amount = m("79228162514264337593543950336.01");
        let json = serde_json::to_string(&amount).expect("serialises");
        assert_eq!(json, "\"79228162514264337593543950336.01\"");
        let back: Money = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, amount);
        // a bare number in the JSON is accepted too
        let from_number: Money = serde_json::from_str("1234.56").expect("number accepted");
        assert_eq!(from_number.to_fixed(2), "1234.56");
        // and so is a very large exponent written as a string
        let scientific: Money = serde_json::from_str("\"1e60\"").expect("exponent accepted");
        // exponents survive the trip intact
        let again: Money =
            serde_json::from_str(&serde_json::to_string(&scientific).unwrap()).unwrap();
        assert_eq!(again, scientific);
        assert_eq!(
            scientific.plus(&Money::from_i64(-1)).to_fixed(0),
            "9".repeat(60)
        );
    }

    #[test]
    fn readme_example_holds() {
        let balance = m("123456789012345678901234567890.55");
        let rate = m("0.075");
        let interest = balance.percentage(&rate, 2);
        assert_eq!(interest.to_fixed(2), "9259259175925925917592592591.79");
        // dividing and rounding each third independently loses a cent; the
        // allocator keeps the total whole
        let weights = [Money::from_i64(1), Money::from_i64(1), Money::from_i64(1)];
        let parts = interest.allocate(&weights, 2);
        assert_eq!(Money::sum(&parts).to_fixed(2), interest.to_fixed(2));
        assert_eq!(parts[0].to_fixed(2), "3086419725308641972530864197.27");
    }

    #[test]
    fn rejects_non_finite_and_garbage() {
        assert!(Money::from_f64_shortest(f64::NAN).is_err());
        assert!(Money::from_f64_shortest(f64::INFINITY).is_err());
        assert!(Money::parse("twelve").is_err());
        assert!(Money::parse("").is_err());
    }

    #[test]
    fn float_bridge_uses_the_shortest_form() {
        assert_eq!(Money::from_f64_shortest(0.1).unwrap().to_fixed(2), "0.10");
        assert_eq!(
            Money::from_f64_shortest(-12.345).unwrap().to_fixed(3),
            "-12.345"
        );
        assert_eq!(
            Money::from_f64_shortest(1e-9).unwrap().to_fixed(9),
            "0.000000001"
        );
    }

    #[test]
    fn comparisons_and_negation() {
        assert!(m("2") > m("1.999999999999999999999"));
        assert_eq!(m("-5").abs(), m("5"));
        assert_eq!(m("5").negate(), m("-5"));
        assert!(m("-0.01").is_negative());
        assert!(!Money::zero().is_negative());
        assert!(Money::zero().is_zero());
    }
}
