//! Double-entry validation: the money rules of the ledger kernel.

use serde::{Deserialize, Serialize};

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
    /// Sum drifted beyond the exactness tolerance used internally.
    ToleranceExceeded { diff: f64 },
}

/// Sum tolerance: 1e-9 - far below any cent-level amount, catches
/// representation noise while never excusing a real imbalance.
const EPS: f64 = 1e-9;

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
/// * total debits == total credits (within 1e-9).
pub fn validate_lines(lines: &[JournalLine]) -> ValidationReport {
    let mut errors = Vec::new();

    if lines.len() < 2 {
        errors.push(ValidationError::TooFewLines { lines: lines.len() });
    }

    let mut total_debits = 0.0f64;
    let mut total_credits = 0.0f64;

    for (i, line) in lines.iter().enumerate() {
        let debit_finite = line.debit.is_finite();
        let credit_finite = line.credit.is_finite();
        if !debit_finite || !credit_finite || line.debit < 0.0 || line.credit < 0.0 {
            errors.push(ValidationError::InvalidAmount { line: i });
            continue;
        }
        let moves_debit = line.debit > 0.0;
        let moves_credit = line.credit > 0.0;
        if moves_debit && moves_credit {
            errors.push(ValidationError::LineMovesBothSides { line: i });
            continue;
        }
        if !moves_debit && !moves_credit {
            errors.push(ValidationError::LineMovesNothing { line: i });
            continue;
        }
        total_debits += line.debit;
        total_credits += line.credit;
    }

    // Only judge balance when the individual lines were structurally valid.
    if errors
        .iter()
        .all(|e| matches!(e, ValidationError::TooFewLines { .. }))
        && !lines.is_empty()
    {
        let diff = (total_debits - total_credits).abs();
        if diff > EPS {
            if diff > 0.01 {
                errors.push(ValidationError::Unbalanced {
                    debits: total_debits,
                    credits: total_credits,
                });
            } else {
                errors.push(ValidationError::ToleranceExceeded { diff });
            }
        }
    }

    ValidationReport {
        valid: errors.is_empty(),
        total_debits,
        total_credits,
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
    fn tolerates_representation_noise() {
        // 0.1+0.2 style noise stays under the 1e-9 tolerance.
        let lines = [JournalLine::new(0.1 + 0.2, 0.0), JournalLine::new(0.0, 0.3)];
        let r = validate_lines(&lines);
        assert!(r.valid);
    }
}
