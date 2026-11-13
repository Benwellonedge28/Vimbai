#![forbid(unsafe_code)]
//! partnership-dissolution-service: what a partnership realises when it is wound up and who is owed what.
//!
//! A dissolution is a report, and every figure on it is derived here from what the caller brought:
//!
//! * each asset is realised at its sale proceeds, a shortfall against book value counting as a loss and anything
//!   over it as a profit, with the two totals kept apart rather than netted;
//! * a creditor's settlement is what was paid, defaulting to what was owed, and the discount is whatever the caller
//!   says it received - the service never worked it out from the two amounts;
//! * a partner is owed capital, current account and their share of the result added together, which is worked out
//!   per partner;
//! * the report is stored completed, and a dissolution the caller does not hold is answered as an error body rather
//!   than a miss, as shipped.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, DISSOLUTION_REPORT};

fn money(value: &Value, key: &str) -> Money {
    Money::parse(&decimal(value, key, "0")).unwrap_or_else(|_| Money::zero())
}

/// `POST /dissolve`.
pub fn dissolve(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let partnership_id = text(payload, "partnership_id");
    if partnership_id.is_empty() {
        return Err(refused("partnership_id is required"));
    }

    let mut realized = Vec::new();
    let (mut profit, mut loss, mut proceeds) = (Money::zero(), Money::zero(), Money::zero());
    for asset in payload
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let book = money(&asset, "book_value");
        let sold = money(&asset, "sale_proceeds");
        let (gain, shortfall) = if sold.minus(&book).is_negative() {
            (Money::zero(), book.minus(&sold))
        } else {
            (sold.minus(&book), Money::zero())
        };
        profit = profit.plus(&gain);
        loss = loss.plus(&shortfall);
        proceeds = proceeds.plus(&sold);
        realized.push(json!({
            "asset_id": text(&asset, "asset_id"),
            "asset_name": text(&asset, "asset_name"),
            "book_value": wire(&book),
            "sale_proceeds": wire(&sold),
            "profit": wire(&gain),
            "loss": wire(&shortfall),
        }));
    }

    let mut settlements = Vec::new();
    let mut paid_out = Money::zero();
    for creditor in payload
        .get("creditors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let owed = money(&creditor, "amount");
        let paid = if creditor.get("paid").is_some() {
            money(&creditor, "paid")
        } else {
            owed.clone()
        };
        paid_out = paid_out.plus(&paid);
        settlements.push(json!({
            "creditor_id": text(&creditor, "id"),
            "creditor_name": text(&creditor, "name"),
            "amount_owed": wire(&owed),
            "amount_paid": wire(&paid),
            "discount_received": wire(&money(&creditor, "discount")),
        }));
    }

    let mut claims = Vec::new();
    let mut owed_to_partners = Money::zero();
    for partner in payload
        .get("partners")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let due = money(&partner, "capital")
            .plus(&money(&partner, "current"))
            .plus(&money(&partner, "share"));
        owed_to_partners = owed_to_partners.plus(&due);
        claims.push(json!({
            "partner_id": text(&partner, "id"),
            "partner_name": text(&partner, "name"),
            "capital_balance": wire(&money(&partner, "capital")),
            "current_account_balance": wire(&money(&partner, "current")),
            "share_of_profit_loss": wire(&money(&partner, "share")),
            "total_due": wire(&due),
        }));
    }

    let report = json!({
        "partnership_id": partnership_id,
        "dissolution_date": text(payload, "dissolution_date"),
        "reason": text(payload, "reason"),
        "assets": realized,
        "realization_profit": wire(&profit),
        "realization_loss": wire(&loss),
        "total_assets_realized": wire(&proceeds),
        "creditors": settlements,
        "total_creditors": wire(&paid_out),
        "partners": claims,
        "total_partners_capitals": wire(&owed_to_partners),
        "status": "completed",
    });
    remember(graph, scope, DISSOLUTION_REPORT, report)
}

/// `GET /dissolutions`.
pub fn list_dissolutions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let rows = Records::new(graph, DISSOLUTION_REPORT).list(scope, 2000)?;
    Ok(json!({"dissolutions": rows}))
}

/// `GET /dissolutions/{dissolution_id}`: answered as an error body rather than a miss, as shipped.
pub fn get_dissolution(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    dissolution_id: &str,
) -> Result<Value, StoreError> {
    match Records::new(graph, DISSOLUTION_REPORT).get(scope, dissolution_id) {
        Ok(report) => Ok(report),
        Err(StoreError::NotFound) => Ok(json!({"error": "Not found"})),
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
    fn a_realised_asset_split_keeps_the_profit_and_the_loss_apart() {
        let graph = FakeGraph::returning(vec![json!({"id": "d-1"})]);
        let answer = dissolve(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "dissolution_date": "2026-05-01", "reason": "insolvency",
                    "assets": [
                        {"asset_id": "a-1", "asset_name": "Van", "book_value": "10000", "sale_proceeds": "11500"},
                        {"asset_id": "a-2", "asset_name": "Racking", "book_value": "2000", "sale_proceeds": "1200"}
                    ],
                    "creditors": [{"id": "c-1", "name": "Supplier", "amount": "3000"}],
                    "partners": [{"id": "p-a", "name": "Ana", "capital": "4000", "current": "300", "share": "-500"}]}),
        )
        .expect("dissolved");
        assert_eq!(answer["realization_profit"], json!("1500.000000"));
        assert_eq!(answer["realization_loss"], json!("800.000000"));
        assert_eq!(answer["total_assets_realized"], json!("12700.000000"));
        assert_eq!(answer["total_creditors"], json!("3000.000000"));
        assert_eq!(answer["partners"][0]["total_due"], json!("3800.000000"));
        assert_eq!(answer["status"], json!("completed"));
    }

    #[test]
    fn an_asset_sold_at_book_value_is_neither_a_profit_nor_a_loss() {
        let graph = FakeGraph::returning(vec![json!({"id": "d-2"})]);
        let answer = dissolve(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "dissolution_date": "2026-05-01", "reason": "agreement",
                    "assets": [{"asset_id": "a-1", "asset_name": "Van", "book_value": "5000", "sale_proceeds": "5000"}],
                    "creditors": [], "partners": []}),
        )
        .expect("dissolved");
        assert_eq!(answer["realization_profit"], json!("0.000000"));
        assert_eq!(answer["realization_loss"], json!("0.000000"));
    }

    #[test]
    fn a_creditor_paid_less_carries_the_discount_it_was_given() {
        let graph = FakeGraph::returning(vec![json!({"id": "d-3"})]);
        let answer = dissolve(
            &graph,
            &scope(),
            &json!({"partnership_id": "p-1", "dissolution_date": "2026-05-01", "reason": "insolvency",
                    "assets": [],
                    "creditors": [{"id": "c-1", "name": "Supplier", "amount": "3000", "paid": "2500",
                                   "discount": "500"}],
                    "partners": []}),
        )
        .expect("dissolved");
        assert_eq!(answer["creditors"][0]["amount_paid"], json!("2500.000000"));
        assert_eq!(
            answer["creditors"][0]["discount_received"],
            json!("500.000000")
        );
        assert_eq!(answer["total_creditors"], json!("2500.000000"));
    }

    #[test]
    fn a_dissolution_the_caller_does_not_hold_is_an_error_body_not_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        let answer = get_dissolution(&graph, &scope(), "d-9").expect("answered");
        assert_eq!(answer["error"], json!("Not found"));
    }

    #[test]
    fn a_dissolution_without_a_partnership_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        assert!(dissolve(&graph, &scope(), &json!({"assets": []})).is_err());
    }
}
