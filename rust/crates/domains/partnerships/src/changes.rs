#![forbid(unsafe_code)]
//! partnership-changes-service: what happens when a partner joins, leaves, or dies.
//!
//! A change is a record: which partnership, which partner, what they were owed, and whether it has been settled. The
//! money the departing partner is owed is worked out here, never taken from the caller, so the record and its total
//! cannot disagree.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an admission's journal drafts are kept with the change: one for the capital brought in, one for goodwill when
//!   any was paid, and the capital draft is skipped when the partner brought nothing;
//! * a retirement is owed capital, current account and goodwill added together, and its single draft debits both
//!   balances and credits cash with that total;
//! * a death is owed capital and current account, with no goodwill added, and its note names the executor;
//! * settling a change the caller does not hold is a miss, and settling twice is allowed - the service never checked
//!   the state it was coming from;
//! * the listing filters by change type and by partnership, reporting rows under `changes` with their count.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, PARTNER_CHANGE};

fn money(record: &Value, key: &str) -> Money {
    Money::parse(&decimal(record, key, "0")).unwrap_or_else(|_| Money::zero())
}

/// The first eight characters a journal reference carries; ids shorter than that are used whole.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// `POST /changes/admission`.
pub fn admit(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "new_partner_name");
    if name.is_empty() {
        return Err(refused("new_partner_name is required"));
    }
    let partner_id = text(payload, "new_partner_id");
    let admitted_on = text(payload, "admission_date");
    let capital = money(payload, "capital_contribution");
    let goodwill = money(payload, "goodwill_paid");

    let mut drafts = Vec::new();
    if !capital.is_zero() && !capital.is_negative() {
        drafts.push(json!({
            "date": admitted_on,
            "description": format!("Capital contribution by {name}"),
            "entries": [
                {"account_code": "1000", "description": "Cash/Bank", "debit": wire(&capital), "credit": "0"},
                {"account_code": "3000", "description": format!("Partner Capital - {name}"),
                 "debit": "0", "credit": wire(&capital)}
            ],
            "reference": format!("ADM-{}", short(&partner_id)),
        }));
    }
    if !goodwill.is_zero() && !goodwill.is_negative() {
        drafts.push(json!({
            "date": admitted_on,
            "description": "Goodwill premium",
            "entries": [
                {"account_code": "1000", "description": "Cash/Bank", "debit": wire(&goodwill), "credit": "0"},
                {"account_code": "1500", "description": "Goodwill", "debit": "0", "credit": wire(&goodwill)}
            ],
            "reference": format!("GW-{}", short(&partner_id)),
        }));
    }

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.insert("journal_entries".into(), Value::Array(drafts));
        // nothing was posted by this engine, so the ids it would have collected stay empty
        fields.insert("journal_entry_ids".into(), json!([]));
    }
    remember(graph, scope, PARTNER_CHANGE, body)
}

/// `POST /changes/retirement`.
pub fn retire(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "partner_name");
    if name.is_empty() {
        return Err(refused("partner_name is required"));
    }
    let partner_id = text(payload, "partner_id");
    let capital = money(payload, "capital_balance");
    let current = money(payload, "current_account_balance");
    let goodwill = money(payload, "goodwill_amount");
    let payable = capital.plus(&current).plus(&goodwill);

    let draft = json!({
        "date": text(payload, "effective_date"),
        "description": format!("Retirement of partner: {name}"),
        "entries": [
            {"account_code": "3000", "description": "Partner Capital",
             "debit": wire(&capital), "credit": "0"},
            {"account_code": "3100", "description": "Partner Current Account",
             "debit": wire(&current), "credit": "0"},
            {"account_code": "1000", "description": "Cash/Bank", "debit": "0", "credit": wire(&payable)}
        ],
        "reference": format!("RET-{}", short(&partner_id)),
    });

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.insert("change_type".into(), json!("retirement"));
        fields.insert("total_payable".into(), wire(&payable));
        fields.insert("journal_entries".into(), json!([draft]));
        fields.insert("journal_entry_ids".into(), json!([]));
    }
    remember(graph, scope, PARTNER_CHANGE, body)
}

/// `POST /changes/death`.
pub fn record_death(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "partner_name");
    if name.is_empty() {
        return Err(refused("partner_name is required"));
    }
    let capital = money(payload, "capital_balance");
    // no goodwill on a death: what is owed is the two balances
    let payable = capital.plus(&money(payload, "current_account_balance"));

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.insert("change_type".into(), json!("death"));
        fields.insert("total_payable".into(), wire(&payable));
        fields.insert(
            "notes".into(),
            json!(format!(
                "Payable to executor: {}",
                text(payload, "executor_name")
            )),
        );
    }
    remember(graph, scope, PARTNER_CHANGE, body)
}

/// `GET /changes`.
pub fn list_changes(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted_type = optional(query, "change_type");
    let wanted_partnership = optional(query, "partnership_id");
    let changes: Vec<Value> = Records::new(graph, PARTNER_CHANGE)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            wanted_type
                .as_deref()
                .map(|want| field(row, "change_type") == want)
                .unwrap_or(true)
                && wanted_partnership
                    .as_deref()
                    .map(|want| field(row, "partnership_id") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(json!({"changes": changes, "count": changes.len()}))
}

/// `POST /changes/{change_id}/settle`: no check on the state it is coming from.
pub fn settle_change(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    change_id: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, PARTNER_CHANGE);
    records.get(scope, change_id)?;
    records.update(scope, change_id, &json!({"settlement_status": "settled"}))?;
    records.get(scope, change_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_retirement_is_owed_capital_current_account_and_goodwill() {
        let graph = FakeGraph::returning(vec![json!({"id": "ch-1"})]);
        let answer = retire(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "partner_id": "1234567890", "partner_name": "Ana",
                    "effective_date": "2026-02-01", "capital_balance": "12000",
                    "current_account_balance": "500", "goodwill_amount": "1500"}),
        )
        .expect("recorded");
        assert_eq!(answer["total_payable"], json!("14000.000000"));
        assert_eq!(answer["change_type"], json!("retirement"));
        assert_eq!(
            answer["journal_entries"][0]["reference"],
            json!("RET-12345678")
        );
        assert_eq!(
            answer["journal_entries"][0]["entries"][2]["credit"],
            json!("14000.000000")
        );
    }

    #[test]
    fn a_death_is_owed_the_two_balances_and_names_the_executor() {
        let graph = FakeGraph::returning(vec![json!({"id": "ch-2"})]);
        let answer = record_death(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "partner_id": "p-9", "partner_name": "Bongani",
                    "effective_date": "2026-03-01", "capital_balance": "8000",
                    "current_account_balance": "250", "executor_name": "Chen"}),
        )
        .expect("recorded");
        assert_eq!(answer["total_payable"], json!("8250.000000"));
        assert_eq!(answer["notes"], json!("Payable to executor: Chen"));
    }

    #[test]
    fn an_admission_drafts_the_capital_and_the_goodwill_it_was_paid() {
        let graph = FakeGraph::returning(vec![json!({"id": "ch-3"})]);
        let answer = admit(
            &graph,
            &scope(),
            &json!({"new_partner_id": "abcdef123", "new_partner_name": "Chen",
                    "admission_date": "2026-04-01", "capital_contribution": "4000",
                    "goodwill_paid": "600"}),
        )
        .expect("admitted");
        assert_eq!(answer["journal_entries"].as_array().unwrap().len(), 2);
        assert_eq!(
            answer["journal_entries"][1]["reference"],
            json!("GW-abcdef12")
        );
        assert_eq!(answer["journal_entry_ids"], json!([]));
    }

    #[test]
    fn an_admission_without_a_contribution_drafts_only_what_it_was_paid() {
        let graph = FakeGraph::returning(vec![json!({"id": "ch-4"})]);
        let answer = admit(
            &graph,
            &scope(),
            &json!({"new_partner_id": "abcdef123", "new_partner_name": "Chen",
                    "admission_date": "2026-04-01", "capital_contribution": "0",
                    "goodwill_paid": "600"}),
        )
        .expect("admitted");
        assert_eq!(answer["journal_entries"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn the_listing_filters_by_type_and_by_partnership() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "ch-1", "change_type": "retirement", "partnership_id": "p-1"}),
            json!({"id": "ch-2", "change_type": "death", "partnership_id": "p-1"}),
            json!({"id": "ch-3", "change_type": "death", "partnership_id": "p-2"}),
        ]);
        let answer = list_changes(
            &graph,
            &scope(),
            &json!({"change_type": "death", "partnership_id": "p-1"}),
        )
        .expect("listed");
        assert_eq!(answer["count"], json!(1));
        assert_eq!(answer["changes"][0]["id"], json!("ch-2"));
    }

    #[test]
    fn settling_a_change_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            settle_change(&graph, &scope(), "ch-9"),
            Err(StoreError::NotFound)
        ));
    }
}
