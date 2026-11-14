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
//! Beyond what was shipped: the service kept no storage, so its `GET /disposals`
//! answered an empty list no matter what had been posted and its `GET /summary`
//! answered zero totals. Posting a disposal now records it, and those two reads
//! report what the caller actually posted. The entry and journal shapes are the
//! ones the service returned, so a client sees no change to the write.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{stored_amount, wire};
use vimbai_store::read::text;
use vimbai_store::{BookScope, Graph, Records, StoreError, ASSET_DISPOSAL};

/// An amount as it goes on the wire: two places, so a client never reads "5000" in one field and
/// "5000.00" in the next. The figure itself is unchanged; only its rendering is settled.
fn at_two(amount: Money) -> Money {
    Money::parse(&amount.to_fixed(2)).expect("a rendered amount parses back")
}

fn zero() -> Money {
    // scale two so a zero on the wire reads "0.00" like every other amount in the answer
    Money::parse("0.00").expect("a literal zero is an amount")
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
    // compared as exact amounts, not as floats: a disposal of a very large asset must not read as
    // a loss because two figures past double precision came out equal
    let (profit, loss) = if req.disposal_proceeds > net_book_value {
        (req.disposal_proceeds.minus(&net_book_value), zero())
    } else {
        (zero(), net_book_value.minus(&req.disposal_proceeds))
    };

    DisposalEntry {
        asset_id: req.asset_id.clone(),
        asset_name: req.asset_name.clone(),
        asset_code: req.asset_code.clone(),
        disposal_date: req.disposal_date.clone(),
        reason: req.reason.clone(),
        original_cost: at_two(req.original_cost.clone()),
        accumulated_depreciation: at_two(req.accumulated_depreciation.clone()),
        net_book_value: at_two(net_book_value.clone()),
        disposal_proceeds: at_two(req.disposal_proceeds.clone()),
        profit_on_disposal: at_two(profit),
        loss_on_disposal: at_two(loss),
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

/// `/disposal/post`: work out the disposal, record it, and keep the journal it posts.
pub fn post_and_record<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry = calculate_from_json(payload).map_err(StoreError::Invalid)?;
    let journal = post_from_json(&entry).map_err(StoreError::Invalid)?;

    let mut fields = entry.clone();
    if let Some(map) = fields.as_object_mut() {
        map.remove("id");
        map.insert("status".into(), json!("posted"));
        map.insert("journal_entry".into(), journal.clone());
        map.insert(
            "journal_entry_id".into(),
            json!(format!("DISP-{}", text(payload, "asset_id"))),
        );
    }
    Records::new(graph, ASSET_DISPOSAL).create(scope, &fields)
}

/// `/disposals`: everything this book has posted, optionally narrowed by status.
pub fn list_disposals<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, ASSET_DISPOSAL).list(scope, 1000)?;
    let kept = match status.filter(|s| !s.is_empty()) {
        None => rows.clone(),
        Some(wanted) => rows
            .into_iter()
            .filter(|row| text(row, "status") == wanted)
            .collect(),
    };
    Ok(json!({"disposals": kept, "count": kept.len()}))
}

/// `/summary`: what the posted disposals came to.
pub fn summary<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let rows = Records::new(graph, ASSET_DISPOSAL).list(scope, 2000)?;
    let mut proceeds = Money::zero();
    let mut profit = Money::zero();
    let mut loss = Money::zero();
    for row in &rows {
        proceeds =
            proceeds.plus(&stored_amount(row, "disposal_proceeds").unwrap_or_else(|_| zero()));
        profit = profit.plus(&stored_amount(row, "profit_on_disposal").unwrap_or_else(|_| zero()));
        loss = loss.plus(&stored_amount(row, "loss_on_disposal").unwrap_or_else(|_| zero()));
    }
    Ok(json!({
        "total_disposals": rows.len(),
        "total_proceeds": wire(&proceeds),
        "total_profit": wire(&profit),
        "total_loss": wire(&loss),
        "net_gain": wire(&profit.minus(&loss)),
    }))
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
