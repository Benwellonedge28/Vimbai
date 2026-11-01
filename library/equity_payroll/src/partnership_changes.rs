//! partnership-changes-service: retirement, admission and death,
//! returning the journals the accounting service would post.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JournalEntry {
    pub account_code: String,
    pub description: String,
    pub debit: f64,
    pub credit: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetirementResult {
    pub partner_name: String,
    pub total_payable: f64,
    pub goodwill_amount: f64,
    pub payment_method: String,
    pub entries: Vec<JournalEntry>,
    pub reference: String,
}

/// Total payable is capital + current account + goodwill; the entry
/// debits both accounts and credits the payment method's bank line.
pub fn record_retirement(
    partner_id: &str,
    partner_name: &str,
    capital_balance: f64,
    current_account_balance: f64,
    goodwill_amount: f64,
    payment_method: &str,
) -> RetirementResult {
    let total_payable = capital_balance + current_account_balance + goodwill_amount;
    RetirementResult {
        partner_name: partner_name.to_string(),
        total_payable,
        goodwill_amount,
        payment_method: payment_method.to_string(),
        entries: vec![
            JournalEntry {
                account_code: "3000".into(),
                description: "Partner Capital".into(),
                debit: capital_balance,
                credit: 0.0,
            },
            JournalEntry {
                account_code: "3100".into(),
                description: "Partner Current Account".into(),
                debit: current_account_balance,
                credit: 0.0,
            },
            JournalEntry {
                account_code: "1000".into(),
                description: "Cash/Bank".into(),
                debit: 0.0,
                credit: total_payable,
            },
        ],
        reference: format!("RET-{}", short(partner_id)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionResult {
    pub new_partner_name: String,
    pub capital_contribution: f64,
    pub goodwill_paid: f64,
    pub entries: Vec<Vec<JournalEntry>>,
}

/// Admission posts a capital contribution journal when there is a
/// contribution and a goodwill premium journal when goodwill is paid.
pub fn record_admission(
    new_partner_id: &str,
    new_partner_name: &str,
    capital_contribution: f64,
    goodwill_paid: f64,
) -> AdmissionResult {
    let _ = short(new_partner_id);
    let mut entries: Vec<Vec<JournalEntry>> = Vec::new();
    if capital_contribution > 0.0 {
        entries.push(vec![
            JournalEntry {
                account_code: "1000".into(),
                description: "Cash/Bank".into(),
                debit: capital_contribution,
                credit: 0.0,
            },
            JournalEntry {
                account_code: "3000".into(),
                description: format!("Partner Capital - {new_partner_name}"),
                debit: 0.0,
                credit: capital_contribution,
            },
        ]);
    }
    if goodwill_paid > 0.0 {
        entries.push(vec![
            JournalEntry {
                account_code: "1000".into(),
                description: "Cash/Bank".into(),
                debit: goodwill_paid,
                credit: 0.0,
            },
            JournalEntry {
                account_code: "1500".into(),
                description: "Goodwill".into(),
                debit: 0.0,
                credit: goodwill_paid,
            },
        ]);
    }
    AdmissionResult {
        new_partner_name: new_partner_name.to_string(),
        capital_contribution,
        goodwill_paid,
        entries,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeathResult {
    pub partner_name: String,
    pub executor_name: String,
    pub total_payable: f64,
    pub notes: String,
}

/// Death settles capital + current account only; no goodwill.
pub fn record_death(
    partner_name: &str,
    capital_balance: f64,
    current_account_balance: f64,
    executor_name: &str,
) -> DeathResult {
    let total_payable = capital_balance + current_account_balance;
    DeathResult {
        partner_name: partner_name.to_string(),
        executor_name: executor_name.to_string(),
        total_payable,
        notes: format!("Payable to executor: {executor_name}"),
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retirement_totals_include_goodwill() {
        let r = record_retirement("p1", "Tendai", 40_000.0, 5_000.0, 10_000.0, "cash");
        assert_eq!(r.total_payable, 55_000.0);
        assert_eq!(r.entries.len(), 3);
        assert_eq!(r.entries[2].credit, 55_000.0);
        assert_eq!(r.reference, "RET-p1");
    }

    #[test]
    fn retirement_without_goodwill_credits_capital_plus_current() {
        let r = record_retirement("partner-2", "Rudo", 12_000.0, -2_000.0, 0.0, "bank");
        assert_eq!(r.total_payable, 10_000.0);
    }

    #[test]
    fn admission_skips_zero_lines() {
        let zero = record_admission("np", "Chipo", 0.0, 0.0);
        assert!(zero.entries.is_empty());
        let both = record_admission("np", "Chipo", 30_000.0, 8_000.0);
        assert_eq!(both.entries.len(), 2);
        assert_eq!(both.entries[1][1].account_code, "1500");
        assert_eq!(both.entries[1][0].debit, 8_000.0);
    }

    #[test]
    fn death_excludes_goodwill() {
        let d = record_death("Farai", 25_000.0, 3_000.0, "Nyarai");
        assert_eq!(d.total_payable, 28_000.0);
        assert_eq!(d.notes, "Payable to executor: Nyarai");
    }
}
