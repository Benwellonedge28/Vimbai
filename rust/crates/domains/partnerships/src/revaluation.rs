#![forbid(unsafe_code)]
//! partnership-revaluation-service: assets written up or down, and how goodwill is treated alongside them.
//!
//! A revaluation is a report of what moved and by how much. As shipped:
//!
//! * an asset is written up when its new value is over its old one, and down otherwise - an asset revalued to
//!   exactly its old value therefore counts as a decrease of nothing, which is why a no-change asset appears under
//!   the decreases rather than being left off;
//! * the upward and downward totals are kept apart, and the net gain is the increase less the decrease;
//! * goodwill raises a reserve only when the treatment is `raise_and_raise` and an amount was given; any other
//!   treatment keeps the amount on the report without drafting anything for it;
//! * the drafts are kept with the report; posting them is the accounting books' business, so the ids stay empty;
//! * a revaluation the caller does not hold is answered as an error body rather than a miss, as shipped.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, PARTNERSHIP_REVALUATION};

fn money(value: &Value, key: &str) -> Money {
    Money::parse(&decimal(value, key, "0")).unwrap_or_else(|_| Money::zero())
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// `POST /revalue`.
pub fn revalue(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let partnership_id = text(payload, "partnership_id");
    if partnership_id.is_empty() {
        return Err(refused("partnership_id is required"));
    }
    let on = text(payload, "revaluation_date");
    let treatment = text(payload, "goodwill_treatment");
    let goodwill = money(payload, "goodwill_amount");

    let mut entries = Vec::new();
    let mut drafts = Vec::new();
    let (mut increase_total, mut decrease_total) = (Money::zero(), Money::zero());

    for row in payload
        .get("asset_revaluations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let old = money(&row, "old_value");
        let new = money(&row, "new_value");
        let code = text(&row, "account_code");
        let name = text(&row, "asset_name");

        let (increase, decrease) = if new.minus(&old).is_negative() {
            (Money::zero(), old.minus(&new))
        } else {
            (new.minus(&old), Money::zero())
        };
        increase_total = increase_total.plus(&increase);
        decrease_total = decrease_total.plus(&decrease);

        if !increase.is_zero() {
            drafts.push(json!({
                "date": on,
                "description": format!("Revaluation gain - {name}"),
                "entries": [
                    {"account_code": code, "description": name, "debit": wire(&increase), "credit": "0"},
                    {"account_code": "3100", "description": "Revaluation Reserve",
                     "debit": "0", "credit": wire(&increase)}
                ],
                "reference": format!("REV-{}", short(&text(&row, "asset_id"))),
            }));
        } else {
            drafts.push(json!({
                "date": on,
                "description": format!("Revaluation loss - {name}"),
                "entries": [
                    {"account_code": "6200", "description": "Revaluation Loss",
                     "debit": wire(&decrease), "credit": "0"},
                    {"account_code": code, "description": name, "debit": "0", "credit": wire(&decrease)}
                ],
                "reference": format!("REV-{}", short(&text(&row, "asset_id"))),
            }));
        }

        entries.push(json!({
            "asset_id": text(&row, "asset_id"),
            "asset_name": name,
            "asset_code": text(&row, "asset_code"),
            "old_value": wire(&old),
            "new_value": wire(&new),
            "increase": wire(&increase),
            "decrease": wire(&decrease),
            "revaluation_gain": wire(&increase),
            "revaluation_loss": wire(&decrease),
        }));
    }

    if !goodwill.is_zero() && !goodwill.is_negative() && treatment == "raise_and_raise" {
        drafts.push(json!({
            "date": on,
            "description": "Goodwill arising on revaluation",
            "entries": [
                {"account_code": "1500", "description": "Goodwill", "debit": wire(&goodwill), "credit": "0"},
                {"account_code": "3100", "description": "Revaluation Reserve",
                 "debit": "0", "credit": wire(&goodwill)}
            ],
            "reference": format!("GW-REV-{}", on.chars().take(7).collect::<String>()),
        }));
    }

    let report = json!({
        "partnership_id": partnership_id,
        "revaluation_date": on,
        "entries": entries,
        "total_increase": wire(&increase_total),
        "total_decrease": wire(&decrease_total),
        "net_gain": wire(&increase_total.minus(&decrease_total)),
        "goodwill_amount": wire(&goodwill),
        "goodwill_treatment": treatment,
        "journal_entries": drafts,
        "journal_entry_ids": [],
    });
    remember(graph, scope, PARTNERSHIP_REVALUATION, report)
}

/// `GET /revaluations`.
pub fn list_revaluations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(query, "partnership_id");
    let rows: Vec<Value> = Records::new(graph, PARTNERSHIP_REVALUATION)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| wanted.is_empty() || text(row, "partnership_id") == wanted)
        .collect();
    Ok(json!({"revaluations": rows, "count": rows.len()}))
}

/// `GET /revaluations/{revaluation_id}`: an error body rather than a miss, as shipped.
pub fn get_revaluation(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    revaluation_id: &str,
) -> Result<Value, StoreError> {
    match Records::new(graph, PARTNERSHIP_REVALUATION).get(scope, revaluation_id) {
        Ok(report) => Ok(report),
        Err(StoreError::NotFound) => Ok(json!({"error": "Revaluation not found"})),
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn ups_and_downs_are_totalled_apart_and_netted_on_the_report() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-1"})]);
        let answer = revalue(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "revaluation_date": "2026-06-01",
            "goodwill_treatment": "write_off",
            "asset_revaluations": [
                {"asset_id": "a-1", "asset_name": "Premises", "asset_code": "P1",
                 "account_code": "1100", "old_value": "100000", "new_value": "130000"},
                {"asset_id": "a-2", "asset_name": "Van", "asset_code": "V1",
                 "account_code": "1200", "old_value": "20000", "new_value": "14000"}
            ]}),
        )
        .expect("revalued");
        assert_eq!(answer["total_increase"], json!("30000.000000"));
        assert_eq!(answer["total_decrease"], json!("6000.000000"));
        assert_eq!(answer["net_gain"], json!("24000.000000"));
        assert_eq!(answer["journal_entries"].as_array().unwrap().len(), 2);
        assert_eq!(answer["journal_entry_ids"], json!([]));
    }

    #[test]
    fn an_asset_at_its_old_value_is_reported_as_a_decrease_of_nothing() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-2"})]);
        let answer = revalue(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "revaluation_date": "2026-06-01", "goodwill_treatment": "write_off",
                    "asset_revaluations": [{"asset_id": "a-1", "asset_name": "Van", "asset_code": "V1",
                                            "account_code": "1200", "old_value": "9000", "new_value": "9000"}]}),
        )
        .expect("revalued");
        assert_eq!(answer["entries"][0]["decrease"], json!("0.000000"));
        assert_eq!(answer["entries"][0]["increase"], json!("0.000000"));
        assert_eq!(answer["net_gain"], json!("0.000000"));
        assert_eq!(answer["journal_entries"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn goodwill_raises_a_reserve_only_under_raise_and_raise() {
        let graph = FakeGraph::returning(vec![json!({"id": "r-3"}), json!({"id": "r-4"})]);
        let raised = revalue(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "revaluation_date": "2026-06-01",
                    "goodwill_treatment": "raise_and_raise", "goodwill_amount": "5000",
                    "asset_revaluations": []}),
        )
        .expect("revalued");
        assert_eq!(raised["journal_entries"].as_array().unwrap().len(), 1);
        assert_eq!(
            raised["journal_entries"][0]["entries"][1]["credit"],
            json!("5000.000000")
        );

        let written_off = revalue(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "revaluation_date": "2026-06-01",
                    "goodwill_treatment": "write_off", "goodwill_amount": "5000",
                    "asset_revaluations": []}),
        )
        .expect("revalued");
        assert_eq!(written_off["journal_entries"].as_array().unwrap().len(), 0);
        assert_eq!(written_off["goodwill_amount"], json!("5000.000000"));
    }

    #[test]
    fn the_listing_filters_by_partnership() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "r-1", "partnership_id": "p-1"}),
            json!({"id": "r-2", "partnership_id": "p-2"}),
        ]);
        let answer =
            list_revaluations(&graph, &scope(), &json!({"partnership_id": "p-2"})).expect("listed");
        assert_eq!(answer["count"], json!(1));
    }

    #[test]
    fn a_revaluation_the_caller_does_not_hold_is_an_error_body() {
        let graph = FakeGraph::returning(vec![]);
        let answer = get_revaluation(&graph, &scope(), "r-9").expect("answered");
        assert_eq!(answer["error"], json!("Revaluation not found"));
    }
}
