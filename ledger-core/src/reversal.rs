//! Reversal-entry rules: corrections are mirrors, never mutations.
//!
//! Vimbai's ledger entries are immutable. A correction is always a pair:
//! a reversing entry that exactly mirrors the original, optionally
//! followed by a new entry with the corrected facts. This module checks
//! the mirror; uniqueness (one reversal per original) is the writer's
//! DB constraint, checked at the call site.

use serde::{Deserialize, Serialize};

use crate::double_entry::JournalLine;

/// A reversal rule violation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum ReversalError {
    /// The reversal does not move the same accounts.
    AccountSetDiffers {
        original: Vec<String>,
        reversal: Vec<String>,
    },
    /// An account's reversal movement is not the exact mirror.
    NotMirrored {
        account: String,
        original: f64,
        reversal: f64,
    },
    /// Nothing to reverse.
    EmptyOriginal,
}

/// A line paired with the account it moves, for mirror comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountMovement {
    pub account: String,
    pub debit: f64,
    pub credit: f64,
}

/// Result of checking a reversal against its original.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReversalCheck {
    pub valid: bool,
    pub errors: Vec<ReversalError>,
}

/// Validates that `reversal` is the exact mirror of `original`:
/// same accounts, each account's debit/credit swapped and amounts equal.
///
/// Account order does not matter; each account must appear at most once
/// in each list (the caller flattens lines per account before calling).
pub fn check_reversal(original: &[AccountMovement], reversal: &[AccountMovement]) -> ReversalCheck {
    let mut errors = Vec::new();

    if original.is_empty() {
        errors.push(ReversalError::EmptyOriginal);
        return ReversalCheck {
            valid: false,
            errors,
        };
    }

    let orig_accounts: Vec<&str> = original.iter().map(|m| m.account.as_str()).collect();
    let rev_accounts: Vec<&str> = reversal.iter().map(|m| m.account.as_str()).collect();

    let orig_set: std::collections::BTreeSet<&str> = orig_accounts.iter().copied().collect();
    let rev_set: std::collections::BTreeSet<&str> = rev_accounts.iter().copied().collect();
    if orig_set != rev_set {
        errors.push(ReversalError::AccountSetDiffers {
            original: orig_set.into_iter().map(String::from).collect(),
            reversal: rev_set.into_iter().map(String::from).collect(),
        });
        return ReversalCheck {
            valid: false,
            errors,
        };
    }

    for o in original {
        let r = match reversal.iter().find(|m| m.account == o.account) {
            Some(r) => r,
            None => continue, // already reported via AccountSetDiffers
        };
        // Exact mirror: reversal debits what the original credited.
        let mirrored_debit = (o.credit - r.debit).abs() < 1e-9;
        let mirrored_credit = (o.debit - r.credit).abs() < 1e-9;
        if !mirrored_debit || !mirrored_credit {
            errors.push(ReversalError::NotMirrored {
                account: o.account.clone(),
                original: o.debit - o.credit,
                reversal: r.debit - r.credit,
            });
        }
    }

    ReversalCheck {
        valid: errors.is_empty(),
        errors,
    }
}

/// Flattens journal lines (which may share accounts) into one
/// `AccountMovement` per account, summing each side.
pub fn flatten_lines(lines: &[JournalLine], accounts: &[String]) -> Vec<AccountMovement> {
    use std::collections::BTreeMap;
    let mut per_account: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for (i, line) in lines.iter().enumerate() {
        let account = accounts.get(i).cloned().unwrap_or_default();
        let e = per_account.entry(account).or_insert((0.0, 0.0));
        e.0 += line.debit;
        e.1 += line.credit;
    }
    per_account
        .into_iter()
        .map(|(account, (debit, credit))| AccountMovement {
            account,
            debit,
            credit,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original() -> Vec<AccountMovement> {
        vec![
            AccountMovement {
                account: "1010".into(),
                debit: 100.0,
                credit: 0.0,
            },
            AccountMovement {
                account: "4000".into(),
                debit: 0.0,
                credit: 100.0,
            },
        ]
    }

    #[test]
    fn accepts_exact_mirror() {
        let reversal = vec![
            AccountMovement {
                account: "1010".into(),
                debit: 0.0,
                credit: 100.0,
            },
            AccountMovement {
                account: "4000".into(),
                debit: 100.0,
                credit: 0.0,
            },
        ];
        let r = check_reversal(&original(), &reversal);
        assert!(r.valid);
    }

    #[test]
    fn accepts_mirror_regardless_of_order() {
        let reversal = vec![
            AccountMovement {
                account: "4000".into(),
                debit: 100.0,
                credit: 0.0,
            },
            AccountMovement {
                account: "1010".into(),
                debit: 0.0,
                credit: 100.0,
            },
        ];
        let r = check_reversal(&original(), &reversal);
        assert!(r.valid);
    }

    #[test]
    fn rejects_wrong_amount() {
        let reversal = vec![
            AccountMovement {
                account: "1010".into(),
                debit: 0.0,
                credit: 99.0,
            },
            AccountMovement {
                account: "4000".into(),
                debit: 100.0,
                credit: 0.0,
            },
        ];
        let r = check_reversal(&original(), &reversal);
        assert!(!r.valid);
        assert!(matches!(r.errors[0], ReversalError::NotMirrored { .. }));
    }

    #[test]
    fn rejects_direction_not_swapped() {
        let reversal = vec![
            AccountMovement {
                account: "1010".into(),
                debit: 100.0,
                credit: 0.0,
            },
            AccountMovement {
                account: "4000".into(),
                debit: 0.0,
                credit: 100.0,
            },
        ];
        let r = check_reversal(&original(), &reversal);
        assert!(!r.valid);
    }

    #[test]
    fn rejects_account_set_change() {
        let reversal = vec![AccountMovement {
            account: "1010".into(),
            debit: 0.0,
            credit: 100.0,
        }];
        let r = check_reversal(&original(), &reversal);
        assert!(!r.valid);
        assert!(matches!(
            r.errors[0],
            ReversalError::AccountSetDiffers { .. }
        ));
    }

    #[test]
    fn rejects_empty_original() {
        let r = check_reversal(&[], &[]);
        assert!(!r.valid);
        assert!(matches!(r.errors[0], ReversalError::EmptyOriginal));
    }

    #[test]
    fn flatten_sums_shared_accounts() {
        let lines = [
            JournalLine::new(50.0, 0.0),
            JournalLine::new(25.0, 0.0),
            JournalLine::new(0.0, 75.0),
        ];
        let accounts = vec!["1010".to_string(), "1010".to_string(), "4000".to_string()];
        let flat = flatten_lines(&lines, &accounts);
        assert_eq!(flat.len(), 2);
        assert_eq!(
            flat[0],
            AccountMovement {
                account: "1010".into(),
                debit: 75.0,
                credit: 0.0
            }
        );
        assert_eq!(
            flat[1],
            AccountMovement {
                account: "4000".into(),
                debit: 0.0,
                credit: 75.0
            }
        );
    }
}
