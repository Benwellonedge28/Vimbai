#![forbid(unsafe_code)]
//! Fixed asset disposals: the profit or loss on disposal, and the journal that
//! the disposal account posts.
//!
//! Ported from disposal-account-service. Amounts are `Money`.
//!
//! Kept as shipped:
//!
//! * Net book value is cost less accumulated depreciation, whatever the reason
//!   for the disposal.
//! * Proceeds above net book value are a profit; anything else - including
//!   exactly equal - is a loss of the difference, which is zero at equality.
//! * The journal debits cash for the proceeds and debits accumulated
//!   depreciation, credits the asset at cost, and credits profit (7100) or
//!   debits loss (7200) to balance.
//! * The reference is `DISP-` plus the first eight characters of the asset id.
//!
//! Not ported: the service kept no storage at all - its `GET /disposals` and
//! `GET /summary` answered a hard-coded empty list and zero totals - so there is
//! nothing to serve. Disposal records belong in the store, which is a separate
//! piece of work from this arithmetic.

use serde::{Deserialize, Serialize};
use vimbai_common::Money;

fn zero() -> Money {
    Money::zero()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisposalRequest {
    pub asset_id: String,
    #[serde(default)]
    pub asset_name: String,
    #[serde(default)]
    pub asset_code: String,
    #[serde(default)]
    pub disposal_date: String,
    #[serde(default)]
    pub reason: String,
    pub original_cost: Money,
    #[serde(default = "zero")]
    pub accumulated_depreciation: Money,
    #[serde(default = "zero")]
    pub disposal_proceeds: Money,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisposalEntry {
    pub asset_id: String,
    pub asset_name: String,
    pub asset_code: String,
    pub disposal_date: String,
    pub reason: String,
    pub original_cost: Money,
    pub accumulated_depreciation: Money,
    pub net_book_value: Money,
    pub disposal_proceeds: Money,
    pub profit_on_disposal: Money,
    pub loss_on_disposal: Money,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

pub fn calculate_asset_disposal(req: &DisposalRequest) -> DisposalEntry {
    let net_book_value = req.original_cost.minus(&req.accumulated_depreciation);
    let (profit, loss) = if req.disposal_proceeds.to_f64_lossy() > net_book_value.to_f64_lossy() {
        (req.disposal_proceeds.minus(&net_book_value), Money::zero())
    } else {
        (Money::zero(), net_book_value.minus(&req.disposal_proceeds))
    };

    DisposalEntry {
        asset_id: req.asset_id.clone(),
        asset_name: req.asset_name.clone(),
        asset_code: req.asset_code.clone(),
        disposal_date: req.disposal_date.clone(),
        reason: req.reason.clone(),
        original_cost: req.original_cost.round(2),
        accumulated_depreciation: req.accumulated_depreciation.round(2),
        net_book_value: net_book_value.round(2),
        disposal_proceeds: req.disposal_proceeds.round(2),
        profit_on_disposal: profit.round(2),
        loss_on_disposal: loss.round(2),
        status: "pending".to_string(),
        notes: req.notes.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JournalLine {
    pub account_code: String,
    pub description: String,
    pub debit: Money,
    pub credit: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisposalJournal {
    pub date: String,
    pub description: String,
    pub reference: String,
    pub entries: Vec<JournalLine>,
}

/// The journal the disposal account posts. Built from the figures the entry
/// already carries, exactly as the service built it.
pub fn post_disposal(entry: &DisposalEntry) -> DisposalJournal {
    let line = |code: &str, description: String, debit: Money, credit: Money| JournalLine {
        account_code: code.to_string(),
        description,
        debit: debit.round(2),
        credit: credit.round(2),
    };

    let mut entries = vec![
        line(
            "1000",
            "Cash/Bank".to_string(),
            entry.disposal_proceeds.clone(),
            Money::zero(),
        ),
        line(
            "1520",
            "Accumulated Depreciation".to_string(),
            entry.accumulated_depreciation.clone(),
            Money::zero(),
        ),
    ];
    entries.push(line(
        "1500",
        format!("Asset Cost - {}", entry.asset_name),
        Money::zero(),
        entry.original_cost.clone(),
    ));

    if entry.profit_on_disposal.to_f64_lossy() > 0.0 {
        entries.push(line(
            "7100",
            "Profit on Disposal".to_string(),
            Money::zero(),
            entry.profit_on_disposal.clone(),
        ));
    } else {
        entries.push(line(
            "7200",
            "Loss on Disposal".to_string(),
            entry.loss_on_disposal.clone(),
            Money::zero(),
        ));
    }

    let reference = format!(
        "DISP-{}",
        entry.asset_id.chars().take(8).collect::<String>()
    );

    DisposalJournal {
        date: entry.disposal_date.clone(),
        description: format!("Disposal of {} - {}", entry.asset_name, entry.reason),
        reference,
        entries,
    }
}

pub fn calculate_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: DisposalRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(calculate_asset_disposal(&request)).map_err(|error| error.to_string())
}

pub fn post_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let entry: DisposalEntry = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(post_disposal(&entry)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn request(cost: &str, accumulated: &str, proceeds: &str) -> DisposalRequest {
        DisposalRequest {
            asset_id: "8f3c1d2e-aaaa-bbbb-cccc-1234567890ab".to_string(),
            asset_name: "Delivery van".to_string(),
            asset_code: "MV-001".to_string(),
            disposal_date: "2026-10-31".to_string(),
            reason: "sold".to_string(),
            original_cost: m(cost),
            accumulated_depreciation: m(accumulated),
            disposal_proceeds: m(proceeds),
            notes: None,
        }
    }

    #[test]
    fn net_book_value_is_cost_less_accumulated_depreciation() {
        let entry = calculate_asset_disposal(&request("50000", "30000", "25000"));
        assert_eq!(entry.net_book_value.to_fixed(2), "20000.00");
        assert_eq!(entry.profit_on_disposal.to_fixed(2), "5000.00");
        assert_eq!(entry.loss_on_disposal, Money::zero());
        assert_eq!(entry.status, "pending");
    }

    #[test]
    fn proceeds_below_net_book_value_are_a_loss() {
        let entry = calculate_asset_disposal(&request("50000", "30000", "15000"));
        assert_eq!(entry.loss_on_disposal.to_fixed(2), "5000.00");
        assert_eq!(entry.profit_on_disposal, Money::zero());
    }

    #[test]
    fn proceeds_equal_to_net_book_value_report_a_zero_loss() {
        let entry = calculate_asset_disposal(&request("50000", "30000", "20000"));
        assert_eq!(entry.profit_on_disposal, Money::zero());
        assert_eq!(entry.loss_on_disposal, Money::zero());
    }

    #[test]
    fn the_journal_balances_on_a_profit_and_on_a_loss() {
        let profit = post_disposal(&calculate_asset_disposal(&request(
            "50000", "30000", "25000",
        )));
        assert_eq!(profit.reference, "DISP-8f3c1d2e");
        assert_eq!(profit.entries.len(), 4);
        assert_eq!(profit.entries[3].account_code, "7100");
        let debits = Money::sum(profit.entries.iter().map(|e| &e.debit));
        let credits = Money::sum(profit.entries.iter().map(|e| &e.credit));
        assert_eq!(debits.to_fixed(2), "55000.00");
        assert_eq!(credits.to_fixed(2), "55000.00");

        let loss = post_disposal(&calculate_asset_disposal(&request(
            "50000", "30000", "15000",
        )));
        assert_eq!(loss.entries[3].account_code, "7200");
        assert_eq!(loss.entries[3].debit.to_fixed(2), "5000.00");
        let debits = Money::sum(loss.entries.iter().map(|e| &e.debit));
        let credits = Money::sum(loss.entries.iter().map(|e| &e.credit));
        assert_eq!(debits.to_fixed(2), "50000.00");
        assert_eq!(credits.to_fixed(2), "50000.00");
    }

    #[test]
    fn a_scrapped_asset_with_no_proceeds_still_balances() {
        let entry = calculate_asset_disposal(&request("18000", "18000", "0"));
        assert_eq!(entry.net_book_value, Money::zero());
        assert_eq!(entry.loss_on_disposal, Money::zero());
        let journal = post_disposal(&entry);
        let debits = Money::sum(journal.entries.iter().map(|e| &e.debit));
        let credits = Money::sum(journal.entries.iter().map(|e| &e.credit));
        assert_eq!(debits.to_fixed(2), "18000.00");
        assert_eq!(credits.to_fixed(2), "18000.00");
    }

    #[test]
    fn amounts_past_double_precision_survive_the_journal() {
        let entry = calculate_asset_disposal(&request(
            "123456789012345678901.55",
            "23456789012345678900.00",
            "123456789012345678901.55",
        ));
        assert_eq!(entry.net_book_value.to_fixed(2), "100000000000000000001.55");
        assert_eq!(
            entry.profit_on_disposal.to_fixed(2),
            "23456789012345678900.00"
        );
        let journal = post_disposal(&entry);
        let debits = Money::sum(journal.entries.iter().map(|e| &e.debit));
        let credits = Money::sum(journal.entries.iter().map(|e| &e.credit));
        assert_eq!(debits.to_fixed(2), credits.to_fixed(2));
    }

    #[test]
    fn the_doors_read_the_published_shapes() {
        let payload = serde_json::json!({
            "asset_id": "8f3c1d2e-1", "asset_name": "Van", "asset_code": "MV-1",
            "disposal_date": "2026-10-31", "reason": "sold",
            "original_cost": "50000", "accumulated_depreciation": "30000",
            "disposal_proceeds": "25000"
        });
        let entry = calculate_from_json(&payload).expect("door");
        assert_eq!(entry["profit_on_disposal"], "5000.00");

        let journal = post_from_json(&entry).expect("door");
        assert_eq!(journal["reference"], "DISP-8f3c1d2e");
        assert_eq!(journal["entries"][3]["account_code"], "7100");

        let bad =
            calculate_from_json(&serde_json::json!({"asset_id": "A1", "original_cost": "lots"}))
                .expect_err("refused");
        assert!(bad.contains("invalid request"), "{bad}");
    }
}
