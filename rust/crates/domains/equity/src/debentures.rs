#![forbid(unsafe_code)]
//! debentures-service, on the store.
//!
//! Borrowed money, from the class that describes it to the day it is repaid: a class carries the
//! nominal, the issue price, the coupon and the redemption price; issues and redemptions move the
//! counters on that class; interest is accrued per period and paid later.
//!
//! The arithmetic, all exact:
//!
//! * issuing credits debenture stock at the nominal value and books the difference to discount, so an
//!   issue at a premium books a negative discount rather than a separate account,
//! * redeeming debits stock at nominal plus a premium on redemption, and credits bank the price paid,
//! * a period's interest is the outstanding count times nominal times the coupon rate, divided by the
//!   number of periods the payment frequency implies (annual 1, semi-annual 2, quarterly 4, monthly
//!   12, and anything unrecognised is monthly),
//! * tax is a flat twenty percent of that, and the net payment is the rest.
//!
//! Kept as shipped:
//!
//! * Accrual takes the outstanding count **from the caller**, not from the class. A caller that passes
//!   a stale count accrues on it.
//! * The interest record is created before its journal entry is returned, and paying it debits the gross
//!   payable while crediting bank net and tax payable the remainder, so the payment entry balances.
//! * Redemption reduces outstanding only; debentures issued keeps the count it had, because it records
//!   what was ever issued.
//! * An unrecognised frequency is treated as monthly rather than refused.
//!
//! One deliberate change: an unknown class or interest record is refused, not answered 200 with an
//! error body.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{
    BookScope, Graph, Records, StoreError, DEBENTURE_CLASS, DEBENTURE_INTEREST, DEBENTURE_ISSUE,
    DEBENTURE_REDEMPTION,
};

/// The flat tax the service assumed on debenture interest.
pub fn tax_rate() -> Money {
    Money::parse("0.2").expect("a ratio")
}

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn count(payload: &Value, key: &str) -> i64 {
    payload
        .get(key)
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(raw) => raw.trim().parse::<i64>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

fn stored_count(record: &Value, key: &str) -> i64 {
    record.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// How many payments a year the frequency implies. An unknown frequency is monthly, as shipped.
pub fn periods_per_year(frequency: &str) -> i64 {
    match frequency {
        "annual" => 1,
        "semi_annual" => 2,
        "quarterly" => 4,
        _ => 12,
    }
}

/// One period's interest on an outstanding count, before tax.
pub fn period_interest(
    nominal: &Money,
    coupon_percent: &Money,
    outstanding: i64,
    periods: i64,
) -> Money {
    let annual = nominal
        .times(&Money::from_i64(outstanding))
        .times(&coupon_percent.over(&Money::from_i64(100)));
    if periods <= 0 {
        return Money::zero();
    }
    annual.over(&Money::from_i64(periods))
}

/// What is withheld and what is actually paid out of that interest.
pub fn split_tax(interest: &Money) -> (Money, Money) {
    let tax = interest.times(&tax_rate());
    (tax.clone(), interest.minus(&tax))
}

/// `/classes/create`.
pub fn create_class<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let nominal = amount_or_zero(payload, "nominal_value")?;
    let issue_price = amount_or_zero(payload, "issue_price")?;
    let coupon = amount_or_zero(payload, "coupon_rate")?;
    let redemption_price = amount_or_zero(payload, "redemption_price")?;
    Records::new(graph, DEBENTURE_CLASS).create(
        scope,
        &json!({
            "name": text(payload, "name"),
            "company_id": text(payload, "company_id"),
            "nominal_value": wire(&nominal),
            "issue_price": wire(&issue_price),
            "coupon_rate": wire(&coupon),
            "interest_payment_frequency": text(payload, "interest_payment_frequency"),
            "maturity_date": text(payload, "maturity_date"),
            "redemption_price": wire(&redemption_price),
            "convertibility": if payload.get("convertibility").is_some() {
                text(payload, "convertibility")
            } else {
                "none".to_string()
            },
            "conversion_terms": payload.get("conversion_terms").cloned().unwrap_or(Value::Null),
            "debentures_issued": 0,
            "debentures_outstanding": 0,
        }),
    )
}

/// `/classes/{id}/issue`.
pub fn issue<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let class = Records::new(graph, DEBENTURE_CLASS).get(scope, class_id)?;
    let nominal = stored_amount(&class, "nominal_value")?;
    let issue_price = stored_amount(&class, "issue_price")?;
    let name = class
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let issued = count(payload, "debentures_issued");
    let proceeds = issue_price.times(&Money::from_i64(issued));
    let discount = nominal.minus(&issue_price).times(&Money::from_i64(issued));
    let stock = nominal.times(&Money::from_i64(issued));
    let zero = Money::zero();

    let already = stored_count(&class, "debentures_issued");
    let outstanding = stored_count(&class, "debentures_outstanding");
    Records::new(graph, DEBENTURE_CLASS).update(
        scope,
        class_id,
        &json!({
            "debentures_issued": already + issued,
            "debentures_outstanding": outstanding + issued,
        }),
    )?;

    let journal = json!({
        "date": text(payload, "issue_date"),
        "description": format!("Issue of {issued} {name} debentures"),
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": wire(&proceeds), "credit": wire(&zero)},
            {"account_code": "2330", "description": "Debenture Discount", "debit": wire(&discount), "credit": wire(&zero)},
            {"account_code": "2320", "description": "Debenture Stock", "debit": wire(&zero), "credit": wire(&stock)},
        ],
        "reference": "DEB-ISS",
    });

    Records::new(graph, DEBENTURE_ISSUE).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "debenture_class_id": class_id,
            "debentures_issued": issued,
            "issue_date": text(payload, "issue_date"),
            "total_proceeds": wire(&proceeds),
            "discount_on_issue": wire(&discount),
            "journal_entry": journal,
        }),
    )
}

/// `/classes/{id}/interest/accrue`.
pub fn accrue_interest<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let class = Records::new(graph, DEBENTURE_CLASS).get(scope, class_id)?;
    let nominal = stored_amount(&class, "nominal_value")?;
    let coupon = stored_amount(&class, "coupon_rate")?;
    let frequency = class
        .get("interest_payment_frequency")
        .and_then(Value::as_str)
        .unwrap_or("monthly")
        .to_string();
    let name = class
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // the count comes from the caller, as the service took it
    let outstanding = count(payload, "debentures_outstanding");
    let periods = periods_per_year(&frequency);
    let interest = period_interest(&nominal, &coupon, outstanding, periods);
    let (tax, net) = split_tax(&interest);
    let zero = Money::zero();

    let journal = json!({
        "date": text(payload, "period_end"),
        "description": format!("Accrual of {name} debenture interest"),
        "entries": [
            {"account_code": "4100", "description": "Interest Expense", "debit": wire(&interest), "credit": wire(&zero)},
            {"account_code": "2335", "description": "Interest Payable", "debit": wire(&zero), "credit": wire(&interest)},
        ],
        "reference": "DEB-INT",
    });

    Records::new(graph, DEBENTURE_INTEREST).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "debenture_class_id": class_id,
            "period_start": text(payload, "period_start"),
            "period_end": text(payload, "period_end"),
            "debentures_outstanding": outstanding,
            "interest_rate": wire(&coupon),
            "interest_amount": wire(&interest),
            "tax_deducted": wire(&tax),
            "net_payment": wire(&net),
            "status": "accrued",
            "journal_entry": journal,
        }),
    )
}

/// `/interest/{id}/pay`.
pub fn pay_interest<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    interest_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, DEBENTURE_INTEREST);
    let interest = records.get(scope, interest_id)?;
    let gross = stored_amount(&interest, "interest_amount")?;
    let tax = stored_amount(&interest, "tax_deducted")?;
    let net = stored_amount(&interest, "net_payment")?;
    let zero = Money::zero();

    let journal = json!({
        "date": text(payload, "payment_date"),
        "description": "Payment of debenture interest",
        "entries": [
            {"account_code": "2335", "description": "Interest Payable", "debit": wire(&gross), "credit": wire(&zero)},
            {"account_code": "1000", "description": "Bank", "debit": wire(&zero), "credit": wire(&net)},
            {"account_code": "2200", "description": "Tax Payable", "debit": wire(&zero), "credit": wire(&tax)},
        ],
        "reference": "DEB-INT-PAY",
    });

    records.update(
        scope,
        interest_id,
        &json!({
            "payment_date": text(payload, "payment_date"),
            "status": "paid",
            "payment_journal_entry": journal,
        }),
    )
}

/// `/classes/{id}/redeem`.
pub fn redeem<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    class_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let class = Records::new(graph, DEBENTURE_CLASS).get(scope, class_id)?;
    let nominal = stored_amount(&class, "nominal_value")?;
    let redemption_price = stored_amount(&class, "redemption_price")?;
    let name = class
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let redeemed = count(payload, "debentures_redeemed");
    let total = redemption_price.times(&Money::from_i64(redeemed));
    let premium = redemption_price
        .minus(&nominal)
        .times(&Money::from_i64(redeemed));
    let stock = nominal.times(&Money::from_i64(redeemed));
    let zero = Money::zero();

    // outstanding falls; what was ever issued keeps its count
    let outstanding = stored_count(&class, "debentures_outstanding");
    Records::new(graph, DEBENTURE_CLASS).update(
        scope,
        class_id,
        &json!({ "debentures_outstanding": outstanding - redeemed }),
    )?;

    let journal = json!({
        "date": text(payload, "redemption_date"),
        "description": format!("Redemption of {redeemed} {name} debentures"),
        "entries": [
            {"account_code": "2320", "description": "Debenture Stock", "debit": wire(&stock), "credit": wire(&zero)},
            {"account_code": "4100", "description": "Premium on Redemption", "debit": wire(&premium), "credit": wire(&zero)},
            {"account_code": "1000", "description": "Bank", "debit": wire(&zero), "credit": wire(&total)},
        ],
        "reference": "DEB-RED",
    });

    Records::new(graph, DEBENTURE_REDEMPTION).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "debenture_class_id": class_id,
            "debentures_redeemed": redeemed,
            "redemption_date": text(payload, "redemption_date"),
            "redemption_price": wire(&redemption_price),
            "total_proceeds": wire(&total),
            "premium_on_redemption": wire(&premium),
            "journal_entry": journal,
        }),
    )
}

fn by_company<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    company_id: Option<&str>,
) -> Result<Vec<Value>, StoreError> {
    let rows = Records::new(graph, kind).list(scope, 1000)?;
    Ok(match company_id {
        Some(company) => rows
            .into_iter()
            .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company))
            .collect(),
        None => rows,
    })
}

/// `/classes`.
pub fn list_classes<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(json!({
        "debenture_classes": by_company(graph, scope, DEBENTURE_CLASS, company_id)?,
    }))
}

/// `/issues`.
pub fn list_issues<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(json!({
        "issues": by_company(graph, scope, DEBENTURE_ISSUE, company_id)?,
    }))
}

/// `/interest`.
pub fn list_interest<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: Option<&str>,
) -> Result<Value, StoreError> {
    Ok(json!({
        "interest_payments": by_company(graph, scope, DEBENTURE_INTEREST, company_id)?,
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

    fn class_record() -> Value {
        json!({
            "id": "cls-1", "name": "2026 loan stock", "nominal_value": "100",
            "issue_price": "95", "coupon_rate": "8", "redemption_price": "104",
            "interest_payment_frequency": "semi_annual",
            "debentures_issued": 1000, "debentures_outstanding": 800
        })
    }

    #[test]
    fn a_semi_annual_coupon_is_half_the_annual_interest() {
        let per_period = period_interest(&money("100"), &money("8"), 800, 2);
        assert_eq!(wire(&per_period), "3200.000000");
    }

    #[test]
    fn an_unrecognised_frequency_is_treated_as_monthly() {
        assert_eq!(periods_per_year("weekly"), 12);
        assert_eq!(periods_per_year("annual"), 1);
        assert_eq!(periods_per_year("quarterly"), 4);
    }

    #[test]
    fn a_fifth_of_the_coupon_is_withheld_and_the_rest_is_paid() {
        let (tax, net) = split_tax(&money("3200"));
        assert_eq!(wire(&tax), "640.000000");
        assert_eq!(wire(&net), "2560.000000");
    }

    #[test]
    fn issuing_at_a_discount_books_the_difference_and_moves_both_counters() {
        let graph = FakeGraph::queue(vec![
            vec![class_record()],
            vec![json!({"id": "cls-1", "name": "2026 loan stock"})],
            vec![json!({"id": "iss-1"})],
        ]);
        issue(
            &graph,
            &scope(),
            "cls-1",
            &json!({"company_id": "co-1", "debentures_issued": 200, "issue_date": "2026-03-01"}),
        )
        .expect("issued");
        let (_, counters) = graph.statement(1);
        assert_eq!(counters["patch"]["debentures_issued"], json!(1200));
        assert_eq!(counters["patch"]["debentures_outstanding"], json!(1000));
        let (_, params) = graph.statement(2);
        assert_eq!(params["props"]["total_proceeds"], json!("19000.000000"));
        assert_eq!(params["props"]["discount_on_issue"], json!("1000.000000"));
    }

    #[test]
    fn redeeming_reduces_outstanding_and_books_the_premium() {
        let graph = FakeGraph::queue(vec![
            vec![class_record()],
            vec![json!({"id": "cls-1", "name": "2026 loan stock"})],
            vec![json!({"id": "red-1"})],
        ]);
        redeem(
            &graph,
            &scope(),
            "cls-1",
            &json!({"company_id": "co-1", "debentures_redeemed": 300, "redemption_date": "2027-03-01"}),
        )
        .expect("redeemed");
        let (_, counters) = graph.statement(1);
        assert_eq!(counters["patch"]["debentures_outstanding"], json!(500));
        assert!(!counters["patch"]
            .as_object()
            .expect("patch")
            .contains_key("debentures_issued"));
        let (_, params) = graph.statement(2);
        assert_eq!(params["props"]["total_proceeds"], json!("31200.000000"));
        assert_eq!(
            params["props"]["premium_on_redemption"],
            json!("1200.000000")
        );
    }

    #[test]
    fn accruing_uses_the_callers_outstanding_count_not_the_class() {
        let graph = FakeGraph::returning(vec![class_record()]);
        accrue_interest(
            &graph,
            &scope(),
            "cls-1",
            &json!({"company_id": "co-1", "period_start": "2026-01-01",
                    "period_end": "2026-06-30", "debentures_outstanding": 1000}),
        )
        .expect("accrued");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["interest_amount"], json!("4000.000000"));
        assert_eq!(written["tax_deducted"], json!("800.000000"));
        assert_eq!(written["net_payment"], json!("3200.000000"));
        assert_eq!(written["interest_rate"], json!("8.000000"));
    }

    #[test]
    fn interest_on_a_borrowing_past_double_size_keeps_every_digit() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "cls-1", "nominal_value": "123456789012345678901.55",
            "coupon_rate": "8", "interest_payment_frequency": "annual"
        })]);
        accrue_interest(
            &graph,
            &scope(),
            "cls-1",
            &json!({"company_id": "co-1", "debentures_outstanding": 2,
                    "period_start": "2026-01-01", "period_end": "2027-01-01"}),
        )
        .expect("accrued");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["interest_amount"],
            json!("19753086241975308624.248000")
        );
    }

    #[test]
    fn paying_an_interest_record_debits_the_gross_and_credits_net_and_tax() {
        let graph = FakeGraph::queue(vec![
            vec![json!({
                "id": "int-1", "interest_amount": "4000", "tax_deducted": "800",
                "net_payment": "3200"
            })],
            vec![json!({"id": "int-1", "status": "paid"})],
        ]);
        pay_interest(
            &graph,
            &scope(),
            "int-1",
            &json!({"payment_date": "2026-07-01"}),
        )
        .expect("paid");
        let (_, params) = graph.statement(1);
        let lines = params["patch"]["payment_journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[0]["debit"], json!("4000.000000"));
        assert_eq!(lines[1]["credit"], json!("3200.000000"));
        assert_eq!(lines[2]["credit"], json!("800.000000"));
    }

    #[test]
    fn an_unknown_class_is_refused() {
        let graph = FakeGraph::returning(Vec::new());
        let outcome = issue(&graph, &scope(), "nope", &json!({"debentures_issued": 1}));
        assert!(matches!(outcome, Err(StoreError::NotFound)));
    }
}
