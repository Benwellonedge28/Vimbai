//! bonus-shares-service, right-issues-service and
//! retained-profits-service: capital-reserve mechanics.
//!
//! Journals are returned as data so the accounting service (or the
//! seam) posts them; nothing here moves money.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalLine {
    pub account_code: String,
    pub description: String,
    pub debit: f64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub description: String,
    pub entries: Vec<JournalLine>,
    pub reference: String,
}

/// bonus-shares-service `/issue`.
/// Reserve account: share_premium -> 3210, retained_earnings -> 3300,
/// general_reserve -> 3310, anything else defaults to 3300.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BonusIssue {
    pub shares_issued: i64,
    pub nominal_value: f64,
    pub total_nominal_value: f64,
    pub amount_utilized: f64,
    pub source_reserve: String,
    pub journal_entry: JournalEntry,
}

pub fn issue_bonus_shares(
    shares_issued: i64,
    nominal_value: f64,
    source_reserve: &str,
) -> BonusIssue {
    let total = shares_issued as f64 * nominal_value;
    let reserve_account = match source_reserve {
        "share_premium" => "3210",
        "retained_earnings" => "3300",
        "general_reserve" => "3310",
        _ => "3300",
    };

    BonusIssue {
        shares_issued,
        nominal_value,
        total_nominal_value: py_round2(total),
        amount_utilized: py_round2(total),
        source_reserve: source_reserve.to_string(),
        journal_entry: JournalEntry {
            description: format!("Issue of {shares_issued} bonus shares from {source_reserve}"),
            entries: vec![
                JournalLine {
                    account_code: reserve_account.to_string(),
                    description: format!("{source_reserve} Reserve"),
                    debit: py_round2(total),
                    credit: 0.0,
                },
                JournalLine {
                    account_code: "3200".to_string(),
                    description: "Share Capital".to_string(),
                    debit: 0.0,
                    credit: py_round2(total),
                },
            ],
            reference: "BONUS-ISSUE".to_string(),
        },
    }
}

/// right-issues-service `/accept`: bank debit, share capital at nominal,
/// balance to share premium. Negative premium is kept as-is (Python does
/// not guard a discount issue here).
pub fn rights_acceptance(
    shares_accepted: i64,
    issue_price: f64,
    nominal_value: f64,
    right_issue_id: &str,
) -> JournalEntry {
    let acceptance_value = shares_accepted as f64 * issue_price;
    let nominal_total = shares_accepted as f64 * nominal_value;

    JournalEntry {
        description: format!("Acceptance of rights issue - {shares_accepted} shares"),
        entries: vec![
            JournalLine {
                account_code: "1000".to_string(),
                description: "Bank".to_string(),
                debit: acceptance_value,
                credit: 0.0,
            },
            JournalLine {
                account_code: "3200".to_string(),
                description: "Share Capital".to_string(),
                debit: 0.0,
                credit: nominal_total,
            },
            JournalLine {
                account_code: "3210".to_string(),
                description: "Share Premium".to_string(),
                debit: 0.0,
                credit: acceptance_value - nominal_total,
            },
        ],
        reference: format!(
            "RIGHT-ACCEPT-{}",
            &right_issue_id[..right_issue_id.len().min(8)]
        ),
    }
}

/// retained-profits-service `/interim-dividends/declare`: retained
/// earnings debit, dividend payable credit. Declaration is recorded even
/// when it exceeds available retained earnings (Python does not check).
pub fn declare_interim_dividend(amount: f64, dividend_id: &str) -> JournalEntry {
    JournalEntry {
        description: "Declaration of interim dividend".to_string(),
        entries: vec![
            JournalLine {
                account_code: "3300".to_string(),
                description: "Retained Earnings".to_string(),
                debit: amount,
                credit: 0.0,
            },
            JournalLine {
                account_code: "2310".to_string(),
                description: "Dividend Payable".to_string(),
                debit: 0.0,
                credit: amount,
            },
        ],
        reference: format!("INTERIM-DIV-{}", &dividend_id[..dividend_id.len().min(8)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonus_issue_from_premium() {
        let b = issue_bonus_shares(10_000, 1.0, "share_premium");
        assert_eq!(b.total_nominal_value, 10_000.0);
        assert_eq!(b.journal_entry.entries[0].account_code, "3210");
        assert_eq!(b.journal_entry.entries[0].debit, 10_000.0);
        assert_eq!(b.journal_entry.entries[1].credit, 10_000.0);
    }

    #[test]
    fn unknown_reserve_defaults_to_retained_earnings() {
        let b = issue_bonus_shares(5, 2.0, "capital_redemption");
        assert_eq!(b.journal_entry.entries[0].account_code, "3300");
        assert_eq!(b.total_nominal_value, 10.0);
    }

    #[test]
    fn rights_premium_split() {
        let j = rights_acceptance(1_000, 3.0, 1.0, "abcdef123456");
        assert_eq!(j.entries[0].debit, 3_000.0);
        assert_eq!(j.entries[1].credit, 1_000.0);
        assert_eq!(j.entries[2].credit, 2_000.0);
        assert_eq!(j.reference, "RIGHT-ACCEPT-abcdef12");
    }

    #[test]
    fn rights_at_discount_keeps_negative_premium() {
        let j = rights_acceptance(1_000, 0.5, 1.0, "id");
        assert_eq!(j.entries[2].credit, -500.0);
    }

    #[test]
    fn interim_dividend_journal() {
        let j = declare_interim_dividend(12_345.67, "deadbeef99");
        assert_eq!(j.entries[0].debit, 12_345.67);
        assert_eq!(j.entries[1].credit, 12_345.67);
        assert_eq!(j.reference, "INTERIM-DIV-deadbeef");
    }
}
