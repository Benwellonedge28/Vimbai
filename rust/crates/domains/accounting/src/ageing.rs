//! The rules of ageing, written once for both sides of the Book.
//!
//! What the business is owed and what it owes are the same question turned around, and they were answered in two files
//! that each carried their own copy of the bands, the void words, the "least of two amounts" helper and the test for a
//! readable date. Two copies of a boundary is one boundary too many: a change to where "over 90" begins in one file and
//! not the other would have the same debt reported as two different ages on the same day.
//!
//! This module holds what the two readers must agree about. What stays with each reader is what is genuinely different:
//! which documents it reads, whose name it prints, and the sentence a person would send.

use serde_json::{Map, Value};
use vimbai_common::dates::days_between;
use vimbai_common::Money;
use vimbai_store::amounts::wire;

/// The bands a debt falls into once it is late, oldest last, as the report names them.
pub(crate) const BANDS: [&str; 4] = ["1_30", "31_60", "61_90", "over_90"];

/// A document marked in one of the ways a Book says it no longer stands.
pub(crate) fn is_void(status: &str) -> bool {
    matches!(status, "void" | "voided" | "cancelled" | "canceled")
}

/// The band a debt falls into, or none when it is not yet late.
///
/// A debt is late from the day after it fell due: a bill due today is not late today, which is what a person chasing it
/// would say and what keeps the answer from disagreeing with the sentence printed beside it.
pub(crate) fn band(days_late: i64) -> Option<&'static str> {
    match days_late {
        d if d <= 0 => None,
        1..=30 => Some(BANDS[0]),
        31..=60 => Some(BANDS[1]),
        61..=90 => Some(BANDS[2]),
        _ => Some(BANDS[3]),
    }
}

/// Whether a date is one this Book can be aged from.
pub(crate) fn readable(date: &str) -> bool {
    days_between(date, date).is_some()
}

/// The smaller of two amounts, so a payment can be taken from a debt without overrunning it.
pub(crate) fn least(left: &Money, right: &Money) -> Money {
    if left.as_decimal() <= right.as_decimal() {
        left.clone()
    } else {
        right.clone()
    }
}

/// A band total for each band, named as the report prints it, built from the one list of bands.
pub(crate) fn columns(bands: &[(&'static str, Money)]) -> Map<String, Value> {
    bands
        .iter()
        .map(|(band, total)| ((*band).to_string(), wire(total)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_debt_is_late_from_the_day_after_it_fell_due() {
        assert_eq!(band(0), None);
        assert_eq!(band(-5), None);
        assert_eq!(band(1), Some("1_30"));
        assert_eq!(band(30), Some("1_30"));
        assert_eq!(band(31), Some("31_60"));
        assert_eq!(band(60), Some("31_60"));
        assert_eq!(band(61), Some("61_90"));
        assert_eq!(band(90), Some("61_90"));
        assert_eq!(band(91), Some("over_90"));
        assert_eq!(band(3650), Some("over_90"));
    }

    #[test]
    fn every_band_the_report_names_has_a_boundary() {
        // a band nobody can reach would be a column that is always empty
        let reached: Vec<&str> = [1, 31, 61, 91]
            .iter()
            .filter_map(|days| band(*days))
            .collect();
        assert_eq!(reached, BANDS.to_vec());
    }

    #[test]
    fn the_least_of_two_amounts_never_overruns_the_debt() {
        let small = Money::parse("10.00").expect("an amount");
        let large = Money::parse("25.00").expect("an amount");
        assert_eq!(least(&small, &large).as_decimal().to_string(), "10.00");
        assert_eq!(least(&large, &small).as_decimal().to_string(), "10.00");
        assert_eq!(least(&large, &large).as_decimal().to_string(), "25.00");
    }

    #[test]
    fn a_date_this_book_cannot_read_is_not_readable() {
        assert!(readable("2026-11-25"));
        assert!(!readable(""));
        assert!(!readable("the 25th"));
    }

    #[test]
    fn the_ways_a_book_voids_a_document_are_all_read() {
        for word in ["void", "voided", "cancelled", "canceled"] {
            assert!(is_void(word), "{word}");
        }
        assert!(!is_void("posted"));
        assert!(!is_void(""));
    }
}
