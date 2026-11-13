//! Double-entry validation: the money rules of the ledger kernel.
//!
//! Balance is decided in EXACT money, never on a float approximation of it. A line's amount arrives as `f64`
//! because that is what a JSON body carries, but each amount is read back to the decimal it was written as
//! (`Money::from_f64_shortest` recovers `0.1` from `0.1`, and the shortest decimal that round-trips is the
//! amount the caller meant), each is rounded to [`LEDGER_SCALE`] - ten decimal places, below anything any currency
//! states and there only to shed what `f64` itself adds to a figure - the totals are added in `BigDecimal`, and the
//! two sides are compared for equality. There is no money tolerance: one cent out of balance is out of balance. The `f64` fields on the report
//! are a rendering of the exact totals for JSON readers, not the figures the decision was taken on.

use serde::{Deserialize, Serialize};
use vimbai_common::money::Money;

/// One journal line. Amounts are in the ledger's minor-unit-free float form
/// (the caller converts); the validator treats them with an exactness
/// tolerance and rejects NaN/inf outright.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JournalLine {
    pub debit: f64,
    pub credit: f64,
}

impl JournalLine {
    pub fn new(debit: f64, credit: f64) -> Self {
        Self { debit, credit }
    }
}

/// A double-entry rule violation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ValidationError {
    /// A line moved money on both sides at once.
    LineMovesBothSides { line: usize },
    /// A line moved no money at all.
    LineMovesNothing { line: usize },
    /// Negative or non-finite amount.
    InvalidAmount { line: usize },
    /// Fewer than two lines: not an entry.
    TooFewLines { lines: usize },
    /// Debits and credits do not sum to the same total.
    Unbalanced { debits: f64, credits: f64 },
}

/// The scale an amount is read at. No currency states more than four decimal places; ten is chosen so that a figure
/// written as `0.1 + 0.2` reads back as `0.3` (what the caller meant) while a genuine sub-cent difference survives.
pub const LEDGER_SCALE: i64 = 10;

/// Outcome of validating a journal entry's lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub total_debits: f64,
    pub total_credits: f64,
    pub errors: Vec<ValidationError>,
}

/// Validates a candidate journal entry's lines against double-entry rules.
///
/// Rules:
/// * at least two lines;
/// * every line is finite and non-negative on both sides;
/// * a line moves exactly one side (debit XOR credit, and that side > 0);
/// * total debits == total credits, exactly.
pub fn validate_lines(lines: &[JournalLine]) -> ValidationReport {
    // the float form is an adapter: what a JSON body carries, read back to the decimals it was written as
    let mut exact = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if !line.debit.is_finite() || !line.credit.is_finite() {
            return ValidationReport {
                valid: false,
                total_debits: 0.0,
                total_credits: 0.0,
                errors: vec![ValidationError::InvalidAmount { line: i }],
            };
        }
        match (
            Money::from_f64_shortest(line.debit),
            Money::from_f64_shortest(line.credit),
        ) {
            (Ok(debit), Ok(credit)) => {
                exact.push((debit.round(LEDGER_SCALE), credit.round(LEDGER_SCALE)))
            }
            _ => {
                return ValidationReport {
                    valid: false,
                    total_debits: 0.0,
                    total_credits: 0.0,
                    errors: vec![ValidationError::InvalidAmount { line: i }],
                }
            }
        }
    }
    validate_money_lines(&exact)
}

/// The rules themselves, decided on exact amounts. This is the kernel's authority: a caller that already holds
/// `Money` - the services do - comes straight here, and [`validate_lines`] is the `f64` door onto it, so there is one
/// implementation of double entry rather than one per caller.
pub fn validate_money_lines(lines: &[(Money, Money)]) -> ValidationReport {
    let mut errors = Vec::new();

    if lines.len() < 2 {
        errors.push(ValidationError::TooFewLines { lines: lines.len() });
    }

    let mut debits = Money::zero();
    let mut credits = Money::zero();
    let mut amounts_valid = true;

    for (i, (debit, credit)) in lines.iter().enumerate() {
        if debit.is_negative() || credit.is_negative() {
            errors.push(ValidationError::InvalidAmount { line: i });
            amounts_valid = false;
            continue;
        }
        let moves_debit = !debit.is_zero();
        let moves_credit = !credit.is_zero();
        if moves_debit && moves_credit {
            errors.push(ValidationError::LineMovesBothSides { line: i });
            amounts_valid = false;
            continue;
        }
        if !moves_debit && !moves_credit {
            errors.push(ValidationError::LineMovesNothing { line: i });
            amounts_valid = false;
            continue;
        }
        debits = debits.plus(debit);
        credits = credits.plus(credit);
    }

    // Only judge balance when the individual lines were structurally valid: a line that moved nothing, both sides or
    // something unreadable is reported on its own, and its amount is not a figure to weigh the entry against.
    if amounts_valid
        && errors
            .iter()
            .all(|e| matches!(e, ValidationError::TooFewLines { .. }))
        && !lines.is_empty()
        && debits != credits
    {
        errors.push(ValidationError::Unbalanced {
            debits: debits.to_f64_lossy(),
            credits: credits.to_f64_lossy(),
        });
    }

    ValidationReport {
        valid: errors.is_empty(),
        total_debits: debits.to_f64_lossy(),
        total_credits: credits.to_f64_lossy(),
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_balanced_entry() {
        let lines = [JournalLine::new(100.0, 0.0), JournalLine::new(0.0, 100.0)];
        let r = validate_lines(&lines);
        assert!(r.valid);
        assert_eq!(r.total_debits, 100.0);
        assert_eq!(r.total_credits, 100.0);
    }

    #[test]
    fn rejects_unbalanced() {
        let lines = [JournalLine::new(100.0, 0.0), JournalLine::new(0.0, 90.0)];
        let r = validate_lines(&lines);
        assert!(!r.valid);
        assert!(matches!(r.errors[0], ValidationError::Unbalanced { .. }));
    }

    #[test]
    fn rejects_both_sides() {
        let lines = [JournalLine::new(100.0, 100.0), JournalLine::new(0.0, 0.0)];
        let r = validate_lines(&lines);
        assert!(!r.valid);
        assert!(matches!(
            r.errors[0],
            ValidationError::LineMovesBothSides { .. }
        ));
    }

    #[test]
    fn rejects_empty_line() {
        let lines = [JournalLine::new(0.0, 0.0), JournalLine::new(5.0, 0.0)];
        let r = validate_lines(&lines);
        assert!(!r.valid);
    }

    #[test]
    fn rejects_negative_and_nan() {
        let lines = [JournalLine::new(-1.0, 0.0), JournalLine::new(0.0, f64::NAN)];
        let r = validate_lines(&lines);
        assert!(!r.valid);
        assert!(matches!(r.errors[0], ValidationError::InvalidAmount { .. }));
    }

    #[test]
    fn rejects_single_line() {
        let lines = [JournalLine::new(100.0, 0.0)];
        let r = validate_lines(&lines);
        assert!(!r.valid);
        assert!(matches!(r.errors[0], ValidationError::TooFewLines { .. }));
    }

    #[test]
    fn reads_float_noise_as_the_figure_the_caller_meant() {
        // 0.1 + 0.2 is 0.30000000000000004 in f64, and it still balances against 0.3 - the reader sheds what the
        // float added, not what the caller wrote.
        let lines = [JournalLine::new(0.1 + 0.2, 0.0), JournalLine::new(0.0, 0.3)];
        let r = validate_lines(&lines);
        assert!(r.valid, "{:?}", r.errors);
    }

    #[test]
    fn a_cent_out_of_balance_is_out_of_balance() {
        // the old 1e-9 tolerance let a one-cent drift through as a lesser fault; there is no money tolerance now
        let lines = [JournalLine::new(100.0, 0.0), JournalLine::new(0.0, 99.99)];
        let r = validate_lines(&lines);
        assert!(!r.valid);
        assert!(
            matches!(r.errors[0], ValidationError::Unbalanced { .. }),
            "{:?}",
            r.errors
        );
    }

    #[test]
    fn sums_a_long_column_without_drift() {
        // ten thousand lines of 0.01 against one line of 100.00: a float sum drifts here, an exact one cannot
        let mut lines: Vec<JournalLine> =
            (0..10_000).map(|_| JournalLine::new(0.01, 0.0)).collect();
        lines.push(JournalLine::new(0.0, 100.0));
        let r = validate_lines(&lines);
        assert!(r.valid, "{:?}", r.errors);
        assert_eq!(r.total_debits, 100.0);
    }
}
