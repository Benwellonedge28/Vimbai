#![forbid(unsafe_code)]
//! The money of the world, answered once for the whole system.
//!
//! Vimbai sells in many countries, so a currency cannot be a detail each feature works out for itself: how exact an
//! amount is, how it is written, and what one money is worth in another are questions the ledger, the invoicing register,
//! the phone and every report all ask. Answered in one place, a Book in Harare billing in dollars and a Book in Panama
//! billing in balboas behave identically, and adding the next country is a row in the catalogue rather than a change to
//! any posting path.
//!
//! Three rules hold here, and nowhere else:
//!
//! * a code names a currency the world knows, or it is refused - never silently treated as a default;
//! * an amount is exact and is written to that currency's own decimal places, so a yen has no cents and a dinar has three;
//! * a conversion is done at a stated rate in exact decimals and rounded to the money being converted *into*, so the same
//!   figures come out whichever feature asks.

use crate::iso_currencies::{find, IsoCurrency};
use crate::money::Money;

/// The currency with this code, or a refusal that names what to do about it.
pub fn currency(code: &str) -> Result<&'static IsoCurrency, String> {
    let wanted = code.trim().to_uppercase();
    if wanted.is_empty() {
        return Err(
            "A document that names a money must name it by its code, such as USD.".to_string(),
        );
    }
    find(&wanted).ok_or_else(|| {
        format!(
            "{wanted} is not a money the world uses. Pick one of the currencies this Book keeps, or add it from the list."
        )
    })
}

/// How many decimal places this currency's money has: none for a yen, two for a dollar, three for a dinar.
///
/// An unknown code is answered as two, because that is what most money uses, but it is answered rather than refused only
/// so a reader that already validated the code is not forced to handle a failure twice.
pub fn decimal_places(code: &str) -> i64 {
    find(code).map_or(2, |one| one.minor_units)
}

/// An amount written in its currency's own money: rounded to its places, with those places shown.
pub fn written(code: &str, amount: &Money) -> String {
    amount.to_fixed(decimal_places(code))
}

/// What [amount] in [from] is worth in [to] at [rate], rounded to the money being converted into.
///
/// [rate] is how much one unit of [from] is worth in [to]. A rate of nothing or less is refused: a document carried at a
/// rate that is not a rate would post figures nobody can defend.
pub fn converted(from: &str, to: &str, amount: &Money, rate: &Money) -> Result<Money, String> {
    let source = currency(from)?;
    let target = currency(to)?;
    if rate.is_zero() || rate.is_negative() {
        return Err(format!(
            "The rate from {} to {} must be more than nothing.",
            source.code, target.code
        ));
    }
    if source.code == target.code {
        // already in the money being asked for: nothing to convert, and no rounding to lose a digit on
        return Ok(amount.clone());
    }
    Ok(amount.times(rate).round(target.minor_units))
}

/// Whether two codes name the same money, whatever their case or surrounding spaces.
pub fn same(from: &str, to: &str) -> bool {
    from.trim().eq_ignore_ascii_case(to.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_yen_has_no_places_and_a_dinar_has_three() {
        assert_eq!(decimal_places("JPY"), 0);
        assert_eq!(decimal_places("KWD"), 3);
        assert_eq!(decimal_places("usd"), 2);
    }

    #[test]
    fn an_amount_is_written_in_its_own_money() {
        assert_eq!(written("JPY", &Money::parse("1000.75").unwrap()), "1001");
        assert_eq!(written("KWD", &Money::parse("1.23456").unwrap()), "1.235");
        assert_eq!(written("USD", &Money::parse("1000").unwrap()), "1000.00");
    }

    #[test]
    fn a_conversion_lands_on_the_money_being_converted_into() {
        let rate = Money::parse("25.5").unwrap();
        let dollars = Money::parse("100000.00").unwrap();
        let bond = converted("USD", "ZWG", &dollars, &rate).expect("a stated rate converts");
        assert_eq!(bond.to_fixed(2), "2550000.00");

        let yen = converted(
            "USD",
            "JPY",
            &Money::parse("100").unwrap(),
            &Money::parse("157.35").unwrap(),
        )
        .expect("a stated rate converts");
        assert_eq!(
            yen.to_fixed(0),
            "15735",
            "a yen conversion lands on whole yen"
        );
    }

    #[test]
    fn a_rate_that_is_not_a_rate_is_refused() {
        let nothing = Money::zero();
        let refused = converted("USD", "ZWG", &Money::from_i64(10), &nothing);
        assert!(refused.is_err());
    }

    #[test]
    fn an_unknown_money_is_refused_rather_than_assumed() {
        let refused = currency("XYZ");
        assert!(refused.is_err());
        assert!(refused.unwrap_err().contains("XYZ"));
        assert!(
            currency(" usd ").is_ok(),
            "case and spaces do not change the money"
        );
    }

    #[test]
    fn the_same_money_is_left_exactly_as_it_was() {
        let amount = Money::parse("1234.56789").unwrap();
        let same = converted("USD", "usd", &amount, &Money::from_i64(1)).expect("same money");
        assert_eq!(
            same.to_fixed(5),
            "1234.56789",
            "no rounding is imposed between the same money"
        );
    }
}
