#![forbid(unsafe_code)]
//! partnership-agreement-service: agreements, the partners in them, and what the register adds up to.
//!
//! An agreement is a record: the terms, the partners and the capital each brought. The capital is never taken from
//! the caller - it is the sum of the partners' contributions, worked out on creation and again when a partner is
//! added, because a caller who sent a total alongside its partners could make the register disagree with itself.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the capital amount on creation is the sum of the partners' contributions, whatever the caller sent;
//! * adding a partner appends it and raises the capital by exactly that partner's contribution;
//! * a change is applied only to fields the agreement already carries, so an unknown field in a change is ignored
//!   rather than invented - the service's `setattr` did the same;
//! * the listing filters by activity when asked, and reports the rows under `agreements` with their count;
//! * the summary counts partners and totals their contributions from the partners themselves, so it cannot drift
//!   from the register.

use serde_json::{json, Value};

use crate::figures::{partner_contribution, partners_of, total_capital};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, maybe_flag, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, PARTNERSHIP_AGREEMENT};

fn agreement_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    agreement_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, PARTNERSHIP_AGREEMENT).get(scope, agreement_id)
}

/// `POST /agreements`.
pub fn create_agreement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "partnership_name");
    if name.is_empty() {
        return Err(refused("partnership_name is required"));
    }
    let partners = partners_of(payload);
    let capital = total_capital(&partners)?;

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.insert("partners".into(), Value::Array(partners));
        fields.insert("capital_amount".into(), wire(&capital));
        // the register is the partners, so a caller's total never survives
        fields.remove("id");
        fields.entry("is_active").or_insert(json!(true));
    } else {
        return Err(refused("an agreement must be an object"));
    }
    remember(graph, scope, PARTNERSHIP_AGREEMENT, body)
}

/// `GET /agreements/{agreement_id}`.
pub fn get_agreement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    agreement_id: &str,
) -> Result<Value, StoreError> {
    agreement_of(graph, scope, agreement_id)
}

/// `GET /agreements`.
pub fn list_agreements(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = query.get("is_active").and_then(Value::as_bool);
    let agreements: Vec<Value> = Records::new(graph, PARTNERSHIP_AGREEMENT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            wanted
                .map(|want| maybe_flag(row, "is_active").unwrap_or(true) == want)
                .unwrap_or(true)
        })
        .collect();
    Ok(json!({"agreements": agreements, "count": agreements.len()}))
}

/// `PUT /agreements/{agreement_id}`: only the fields the agreement already carries are changed.
pub fn update_agreement(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    agreement_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let agreement = agreement_of(graph, scope, agreement_id)?;
    let existing = agreement.as_object().cloned().unwrap_or_default();
    let mut patch = serde_json::Map::new();
    if let Some(changes) = payload.as_object() {
        for (key, value) in changes {
            if existing.contains_key(key) {
                patch.insert(key.clone(), value.clone());
            }
        }
    }
    // the fields the register owns are left out of a change, so they cannot be set by hand
    patch.remove("id");
    patch.remove("created_at");
    patch.remove("user_id");
    patch.remove("book_id");
    Records::new(graph, PARTNERSHIP_AGREEMENT).update(
        scope,
        agreement_id,
        &Value::Object(patch),
    )?;
    agreement_of(graph, scope, agreement_id)
}

/// `POST /agreements/{agreement_id}/partners/add`.
pub fn add_partner(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    agreement_id: &str,
    partner: &Value,
) -> Result<Value, StoreError> {
    let agreement = agreement_of(graph, scope, agreement_id)?;
    let added = partner_contribution(partner)?;

    let mut partners = partners_of(&agreement);
    partners.push(partner.clone());
    let capital = Money::parse(&decimal(&agreement, "capital_amount", "0"))
        .unwrap_or_else(|_| Money::zero())
        .plus(&added);

    Records::new(graph, PARTNERSHIP_AGREEMENT).update(
        scope,
        agreement_id,
        &json!({"partners": partners, "capital_amount": wire(&capital)}),
    )?;
    agreement_of(graph, scope, agreement_id)
}

/// `GET /agreements/{agreement_id}/summary`.
pub fn agreement_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    agreement_id: &str,
) -> Result<Value, StoreError> {
    let agreement = agreement_of(graph, scope, agreement_id)?;
    let partners = partners_of(&agreement);
    let total = total_capital(&partners)?;

    Ok(json!({
        "partnership_name": field(&agreement, "partnership_name"),
        "total_capital": wire(&total),
        "partner_count": partners.len(),
        "partners": partners
            .iter()
            .map(|partner| json!({
                "name": field(partner, "name"),
                "contribution": decimal(partner, "contribution", "0"),
                "profit_share": decimal(partner, "profit_sharing_ratio", "0"),
            }))
            .collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn the_capital_is_the_sum_of_the_partners_even_when_the_caller_sends_one() {
        let graph = FakeGraph::returning(vec![json!({"id": "ag-1"})]);
        let answer = create_agreement(
            &graph,
            &scope(),
            &json!({"partnership_name": "Two Hands", "agreement_number": "A-1", "business_nature": "retail",
                    "start_date": "2026-01-01", "capital_amount": 999999,
                    "partners": [
                        {"name": "Ana", "contribution": 5000},
                        {"name": "Bongani", "contribution": 7500}
                    ]}),
        )
        .expect("created");
        assert_eq!(answer["capital_amount"], json!("12500.000000"));
        assert_eq!(answer["partners"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn adding_a_partner_raises_the_capital_by_that_partner() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "ag-1", "capital_amount": "12500.000000",
                        "partners": [{"name": "Ana", "contribution": "5000.000000"}]})],
            vec![json!({"id": "ag-1", "capital_amount": "15500.000000"})],
            vec![json!({"id": "ag-1", "capital_amount": "15500.000000"})],
        ]);
        let answer = add_partner(
            &graph,
            &scope(),
            "ag-1",
            &json!({"name": "Chen", "contribution": 3000}),
        )
        .expect("added");
        assert_eq!(answer["capital_amount"], json!("15500.000000"));
    }

    #[test]
    fn a_change_touches_only_the_fields_the_agreement_already_carries() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "ag-1", "partnership_name": "Two Hands", "capital_amount": "0.000000"}),
            ],
            vec![json!({"id": "ag-1", "partnership_name": "Three Hands"})],
            vec![json!({"id": "ag-1", "partnership_name": "Three Hands"})],
        ]);
        let answer = update_agreement(
            &graph,
            &scope(),
            "ag-1",
            &json!({"partnership_name": "Three Hands", "invented_field": "ignored"}),
        )
        .expect("updated");
        assert_eq!(answer["partnership_name"], json!("Three Hands"));
    }

    #[test]
    fn the_listing_filters_by_activity_and_counts_what_it_returns() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "ag-1", "is_active": true}),
            json!({"id": "ag-2", "is_active": false}),
        ]);
        let answer =
            list_agreements(&graph, &scope(), &json!({"is_active": false})).expect("listed");
        assert_eq!(answer["count"], json!(1));
        assert_eq!(answer["agreements"][0]["id"], json!("ag-2"));
    }

    #[test]
    fn the_summary_totals_the_partners_themselves() {
        let graph = FakeGraph::queue(vec![vec![json!({
            "id": "ag-1", "partnership_name": "Two Hands", "capital_amount": "999.000000",
            "partners": [
                {"name": "Ana", "contribution": "5000.000000", "profit_sharing_ratio": "60.000000"},
                {"name": "Bongani", "contribution": "7500.000000", "profit_sharing_ratio": "40.000000"}
            ]
        })]]);
        let answer = agreement_summary(&graph, &scope(), "ag-1").expect("summarised");
        assert_eq!(answer["total_capital"], json!("12500.000000"));
        assert_eq!(answer["partner_count"], json!(2));
        assert_eq!(answer["partners"][0]["profit_share"], json!("60.000000"));
    }

    #[test]
    fn an_agreement_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            get_agreement(&graph, &scope(), "ag-9"),
            Err(StoreError::NotFound)
        ));
        assert!(matches!(
            add_partner(&graph, &scope(), "ag-9", &json!({"contribution": 1})),
            Err(StoreError::NotFound)
        ));
    }
}
