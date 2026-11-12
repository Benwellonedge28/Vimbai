#![forbid(unsafe_code)]
//! capital-reconstruction-service, on the store.
//!
//! A scheme that rewrites the capital structure: share capital is reduced, losses are written off
//! against the reduction, and reserves are reclassified. It is kept as a record of the scheme with its
//! adjustments and conversions, then approved once and executed.
//!
//! The arithmetic, all exact:
//!
//! * the reduction is previous share capital less new share capital,
//! * a transfer adjustment carries a new balance; every other adjustment type leaves it at zero,
//! * approval debits share capital by the reduction and each write-off account by its amount, then
//!   credits retained earnings with the sum - which is what makes the entry balance,
//! * the summary totals the reduction across a company's schemes and counts the completed ones.
//!
//! Kept as shipped, and pinned rather than fixed:
//!
//! * A transfer adjustment's new balance is stored but does not enter the approval entry, so only
//!   write-offs are actually posted.
//! * Conversion entries are returned with the scheme but the approval entry does not post them; each
//!   conversion carries its own journal entry of the move from one account to the other.
//! * Approval can be run more than once: it always sets `approved` first and `completed` after, with no
//!   guard on the prior status, so a second call writes a second journal entry. Closing that here would
//!   change what an existing scheme has already posted.
//!
//! One deliberate change: an unknown scheme is refused rather than answered 200 with an error body.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, CAPITAL_RECONSTRUCTION, RECONSTRUCTION_ADJUSTMENT,
    RESERVE_CONVERSION,
};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// What a scheme reduces share capital by.
pub fn reduction(previous: &Money, new: &Money) -> Money {
    previous.minus(new)
}

/// Where a transfer adjustment leaves the account; every other type reports nothing.
pub fn adjusted_balance(kind: &str, previous: &Money, amount: &Money) -> Money {
    if kind == "transfer" {
        previous.plus(amount)
    } else {
        Money::zero()
    }
}

/// The lines an approval posts: the reduction, each write-off, and the balancing credit.
pub fn approval_lines(reduction: &Money, write_offs: &[(String, String, Money)]) -> Vec<Value> {
    let zero = Money::zero();
    let mut lines = vec![json!({
        "account_code": "3200",
        "description": "Share Capital",
        "debit": wire(reduction),
        "credit": wire(&zero),
    })];
    let mut total = reduction.clone();
    for (code, name, amount) in write_offs {
        lines.push(json!({
            "account_code": code,
            "description": name,
            "debit": wire(amount),
            "credit": wire(&zero),
        }));
        total = total.plus(amount);
    }
    lines.push(json!({
        "account_code": "3300",
        "description": "Retained Earnings / P&L",
        "debit": wire(&zero),
        "credit": wire(&total),
    }));
    lines
}

/// `/reconstructions/create`.
pub fn create_reconstruction<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let previous = amount_or_zero(payload, "previous_share_capital")?;
    let new = amount_or_zero(payload, "new_share_capital")?;
    let reduction_amount = reduction(&previous, &new);
    Records::new(graph, CAPITAL_RECONSTRUCTION).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "reconstruction_type": text(payload, "reconstruction_type"),
            "description": text(payload, "description"),
            "scheme_date": text(payload, "scheme_date"),
            "previous_share_capital": wire(&previous),
            "new_share_capital": wire(&new),
            "share_consolidation_ratio": text(payload, "share_consolidation_ratio"),
            "capital_reduction_amount": wire(&reduction_amount),
            "status": "draft",
        }),
    )
}

/// `/reconstructions/{id}/adjustments/add`.
pub fn add_adjustment<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reconstruction_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    Records::new(graph, CAPITAL_RECONSTRUCTION).get(scope, reconstruction_id)?;
    let previous = amount_or_zero(payload, "previous_balance")?;
    let amount = amount_or_zero(payload, "adjustment_amount")?;
    let kind = text(payload, "adjustment_type");
    let new_balance = adjusted_balance(&kind, &previous, &amount);
    Records::new(graph, RECONSTRUCTION_ADJUSTMENT).create(
        scope,
        &json!({
            "reconstruction_id": reconstruction_id,
            "account_code": text(payload, "account_code"),
            "account_name": text(payload, "account_name"),
            "previous_balance": wire(&previous),
            "adjustment_type": kind,
            "adjustment_amount": wire(&amount),
            "description": text(payload, "description"),
            "new_balance": wire(&new_balance),
        }),
    )
}

/// `/reconstructions/{id}/reserve-conversions/add`.
pub fn add_reserve_conversion<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reconstruction_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    Records::new(graph, CAPITAL_RECONSTRUCTION).get(scope, reconstruction_id)?;
    let amount = amount_or_zero(payload, "amount")?;
    let zero = Money::zero();
    let from_account = text(payload, "from_account");
    let from_name = text(payload, "from_account_name");
    let to_account = text(payload, "to_account");
    let to_name = text(payload, "to_account_name");
    let date = if text(payload, "conversion_date").is_empty() {
        text(payload, "scheme_date")
    } else {
        text(payload, "conversion_date")
    };

    let journal = json!({
        "date": date,
        "description": format!("Reserve conversion: {from_name} to {to_name}"),
        "entries": [
            {
                "account_code": from_account,
                "description": from_name,
                "debit": wire(&amount),
                "credit": wire(&zero),
            },
            {
                "account_code": to_account,
                "description": to_name,
                "debit": wire(&zero),
                "credit": wire(&amount),
            },
        ],
        "reference": "RCONV",
    });

    Records::new(graph, RESERVE_CONVERSION).create(
        scope,
        &json!({
            "reconstruction_id": reconstruction_id,
            "from_account": from_account,
            "from_account_name": from_name,
            "to_account": to_account,
            "to_account_name": to_name,
            "amount": wire(&amount),
            "conversion_date": date,
            "reason": text(payload, "reason"),
            "journal_entry": journal,
        }),
    )
}

/// `/reconstructions/{id}/approve`.
pub fn approve_reconstruction<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reconstruction_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let schemes = Records::new(graph, CAPITAL_RECONSTRUCTION);
    let scheme = schemes.get(scope, reconstruction_id)?;
    let reduction_amount = stored_amount(&scheme, "capital_reduction_amount")?;

    let adjustments: Vec<Value> = Records::new(graph, RECONSTRUCTION_ADJUSTMENT)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            row.get("reconstruction_id").and_then(Value::as_str) == Some(reconstruction_id)
        })
        .collect();
    let conversions: Vec<Value> = Records::new(graph, RESERVE_CONVERSION)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            row.get("reconstruction_id").and_then(Value::as_str) == Some(reconstruction_id)
        })
        .collect();

    let mut write_offs = Vec::new();
    for row in &adjustments {
        if row.get("adjustment_type").and_then(Value::as_str) == Some("write_off") {
            write_offs.push((
                row.get("account_code")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                row.get("account_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                stored_amount(row, "adjustment_amount")?,
            ));
        }
    }

    let journal = json!({
        "date": scheme.get("scheme_date").cloned().unwrap_or(Value::Null),
        "description": format!(
            "Capital reconstruction: {}",
            scheme.get("description").and_then(Value::as_str).unwrap_or_default()
        ),
        "entries": approval_lines(&reduction_amount, &write_offs),
        "reference": format!("RECONST-{reconstruction_id}"),
    });

    let mut patch = serde_json::Map::new();
    patch.insert("journal_entry".into(), journal);
    patch.insert("status".into(), json!("completed"));
    for key in ["court_approval_date", "shareholders_approval_date"] {
        if !text(payload, key).is_empty() {
            patch.insert(key.into(), json!(text(payload, key)));
        }
    }
    let saved = schemes.update(scope, reconstruction_id, &Value::Object(patch))?;

    Ok(json!({
        "reconstruction": saved,
        "adjustments": adjustments,
        "conversions": conversions,
    }))
}

/// `/reconstructions`, by company and status.
pub fn list_reconstructions<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, CAPITAL_RECONSTRUCTION).list(scope, 1000)?;
    let filtered: Vec<Value> = rows
        .into_iter()
        .filter(|row| match company_id {
            Some(company) => row.get("company_id").and_then(Value::as_str) == Some(company),
            None => true,
        })
        .filter(|row| match status {
            Some(wanted) => row.get("status").and_then(Value::as_str) == Some(wanted),
            None => true,
        })
        .collect();
    Ok(json!({ "reconstructions": filtered }))
}

/// `/reconstructions/{id}`.
pub fn reconstruction_detail<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    reconstruction_id: &str,
) -> Result<Value, StoreError> {
    let scheme = Records::new(graph, CAPITAL_RECONSTRUCTION).get(scope, reconstruction_id)?;
    let mine = |rows: Vec<Value>| -> Vec<Value> {
        rows.into_iter()
            .filter(|row| {
                row.get("reconstruction_id").and_then(Value::as_str) == Some(reconstruction_id)
            })
            .collect()
    };
    Ok(json!({
        "reconstruction": scheme,
        "adjustments": mine(Records::new(graph, RECONSTRUCTION_ADJUSTMENT).list(scope, 1000)?),
        "reserve_conversions": mine(Records::new(graph, RESERVE_CONVERSION).list(scope, 1000)?),
    }))
}

/// `/summary/{company_id}`.
pub fn company_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, CAPITAL_RECONSTRUCTION).list(scope, 1000)?;
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    let mut total = Money::zero();
    let mut completed = 0i64;
    for row in &mine {
        total = total.plus(&stored_amount(row, "capital_reduction_amount")?);
        if row.get("status").and_then(Value::as_str) == Some("completed") {
            completed += 1;
        }
    }
    Ok(json!({
        "company_id": company_id,
        "total_reconstructions": mine.len(),
        "completed_reconstructions": completed,
        "total_capital_reduced": wire(&total),
        "reconstructions": mine,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    #[test]
    fn the_reduction_is_the_difference_between_the_two_capitals() {
        assert_eq!(
            wire(&reduction(&money("1000"), &money("400"))),
            "600.000000"
        );
    }

    #[test]
    fn only_a_transfer_carries_a_new_balance() {
        assert_eq!(
            wire(&adjusted_balance("transfer", &money("100"), &money("25"))),
            "125.000000"
        );
        assert_eq!(
            wire(&adjusted_balance("write_off", &money("100"), &money("25"))),
            "0.000000"
        );
    }

    #[test]
    fn an_approval_balances_the_reduction_and_the_write_offs_against_retained_earnings() {
        let lines = approval_lines(
            &money("600"),
            &[(
                "3000".to_string(),
                "Accumulated loss".to_string(),
                money("250"),
            )],
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1]["debit"], json!("250.000000"));
        assert_eq!(lines[2]["credit"], json!("850.000000"));
    }

    #[test]
    fn creating_a_scheme_records_what_it_reduces() {
        let graph = FakeGraph::returning(vec![json!({"id": "rec-1"})]);
        create_reconstruction(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "reconstruction_type": "internal",
                    "description": "Loss write-off", "scheme_date": "2026-03-01",
                    "previous_share_capital": "1000", "new_share_capital": "400"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["capital_reduction_amount"],
            json!("600.000000")
        );
        assert_eq!(params["props"]["status"], json!("draft"));
    }

    #[test]
    fn a_reduction_past_double_size_keeps_every_digit() {
        let graph = FakeGraph::returning(vec![json!({"id": "rec-1"})]);
        create_reconstruction(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "previous_share_capital":
                    "123456789012345678901.55", "new_share_capital": "0.45"}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["capital_reduction_amount"],
            json!("123456789012345678901.100000")
        );
    }

    #[test]
    fn approving_posts_only_the_write_offs_and_balances() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "rec-1", "capital_reduction_amount": "600",
                        "scheme_date": "2026-03-01", "description": "Loss write-off"})],
            vec![
                json!({"id": "adj-1", "reconstruction_id": "rec-1", "adjustment_type": "write_off",
                       "account_code": "3000", "account_name": "Accumulated loss",
                       "adjustment_amount": "250"}),
                json!({"id": "adj-2", "reconstruction_id": "rec-1", "adjustment_type": "transfer",
                       "account_code": "3100", "account_name": "Reserve",
                       "adjustment_amount": "999"}),
            ],
            vec![json!({"id": "conv-1", "reconstruction_id": "rec-1", "amount": "50"})],
            vec![json!({"id": "rec-1", "status": "completed"})],
        ]);
        let answer =
            approve_reconstruction(&graph, &scope(), "rec-1", &json!({})).expect("approved");
        assert_eq!(
            answer["adjustments"].as_array().expect("adjustments").len(),
            2
        );
        assert_eq!(
            answer["conversions"].as_array().expect("conversions").len(),
            1
        );
        let (_, params) = graph.statement(3);
        let lines = params["patch"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2]["credit"], json!("850.000000"));
        assert_eq!(params["patch"]["status"], json!("completed"));
    }

    #[test]
    fn a_conversion_posts_the_move_between_the_two_accounts() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "rec-1"})],
            vec![json!({"id": "conv-1"})],
        ]);
        add_reserve_conversion(
            &graph,
            &scope(),
            "rec-1",
            &json!({"from_account": "3100", "from_account_name": "Share premium",
                    "to_account": "3200", "to_account_name": "Share capital",
                    "amount": "500", "reason": "Bonus issue", "conversion_date": "2026-03-02"}),
        )
        .expect("added");
        let (_, params) = graph.statement(1);
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("500.000000"));
        assert_eq!(lines[1]["credit"], json!("500.000000"));
    }

    #[test]
    fn the_summary_totals_the_reduction_and_counts_what_completed() {
        let graph = FakeGraph::returning(vec![
            json!({"company_id": "co-1", "capital_reduction_amount": "600", "status": "completed"}),
            json!({"company_id": "co-1", "capital_reduction_amount": "150.25", "status": "draft"}),
            json!({"company_id": "co-2", "capital_reduction_amount": "999", "status": "completed"}),
        ]);
        let answer = company_summary(&graph, &scope(), "co-1").expect("summary");
        assert_eq!(answer["total_reconstructions"], json!(2));
        assert_eq!(answer["completed_reconstructions"], json!(1));
        assert_eq!(answer["total_capital_reduced"], json!("750.250000"));
    }
}
