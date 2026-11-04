//! Payment optimisation, ported from payment-optimization-service.
//!
//! Amounts and discounts are decimals, so a portfolio of very large invoices totals
//! exactly. Rates stay two-place, as the service published them.
//!
//! The decision rule is the service's: take a discount when its annualised cost is
//! below 20%, and only then when the cash is actually on hand. A discount that is
//! available but not taken still counts as available, which is why utilisation can
//! sit far below 100%.

use serde_json::{json, Value};
use vimbai_common::Money;

fn money_of(value: Option<&Value>) -> Result<Money, String> {
    match value {
        Some(Value::String(text)) => Money::parse(text).map_err(|e| e.0),
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|e| format!("{}: {e}", number)),
        Some(Value::Null) | None => Ok(Money::zero()),
        Some(other) => Err(format!("expected an amount, found {other}")),
    }
}

fn text_of<'a>(item: &'a Value, key: &str) -> &'a str {
    item.get(key).and_then(Value::as_str).unwrap_or("")
}

/// One payment's schedule line and the discount it contributes.
struct Decision {
    line: Value,
    discount_available: Money,
    discount_taken: Money,
}

fn decide(payment: &Value, available_cash: &Money) -> Result<Decision, String> {
    let amount = money_of(payment.get("amount"))?;
    let discount = money_of(payment.get("discount_available"))?;
    let discount_days = payment
        .get("discount_days")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let rate = discount.ratio_pct(&amount, 10);
    let rate_fraction = rate.over(&Money::from_i64(100));
    let annualised = if discount_days > 0 {
        rate_fraction
            .times(&Money::from_i64(365))
            .over(&Money::from_i64(discount_days))
    } else {
        Money::zero()
    };
    let annualised_pct = annualised.times(&Money::from_i64(100)).round(2);

    let worth_taking = annualised_pct < Money::from_i64(20);
    let paid_early = worth_taking && &amount <= available_cash;

    let (action, payment_date, savings) = if paid_early {
        (
            "PAY_EARLY",
            "Early (discount)".to_string(),
            discount.clone(),
        )
    } else {
        (
            "PAY_NORMAL",
            text_of(payment, "due_date").to_string(),
            Money::zero(),
        )
    };

    Ok(Decision {
        line: json!({
            "payment_id": text_of(payment, "payment_id"),
            "amount": amount.to_fixed(2),
            "due_date": text_of(payment, "due_date"),
            "payment_date": payment_date,
            "action": action,
            "discount_available": discount.to_fixed(2),
            "savings": savings.round(2).to_fixed(2),
            "annualized_cost_pct": annualised_pct.to_fixed(2),
        }),
        discount_available: discount,
        discount_taken: if paid_early { savings } else { Money::zero() },
    })
}

/// The payment schedule, the discount it captures, and what to do about it.
pub fn evaluate(payload: &Value) -> Result<Value, String> {
    let available_cash = money_of(payload.get("available_cash"))?;
    let payments = payload
        .get("payments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut schedule = Vec::with_capacity(payments.len());
    let mut available = Money::zero();
    let mut taken = Money::zero();
    let mut amount_total = Money::zero();

    for payment in &payments {
        let decision = decide(payment, &available_cash)?;
        available = available.plus(&decision.discount_available);
        taken = taken.plus(&decision.discount_taken);
        amount_total = amount_total.plus(&money_of(payment.get("amount"))?);
        schedule.push(decision.line);
    }

    let utilisation = taken.ratio_pct(&available, 2);

    let mut recommendations = Vec::new();
    if utilisation < Money::from_i64(50) {
        recommendations
            .push("Low discount utilization - consider paying invoices early to capture discounts");
    }
    let threshold = amount_total
        .times(&Money::from_i64(30))
        .over(&Money::from_i64(100));
    if available_cash < threshold {
        recommendations.push("Limited cash - prioritize high-value discounts");
    }

    Ok(json!({
        "company_id": text_of(payload, "company_id"),
        "payment_schedule": schedule,
        "savings_analysis": {
            "total_payments": payments.len(),
            "total_amount": amount_total.round(2).to_fixed(2),
            "discount_available": available.round(2).to_fixed(2),
            "discount_taken": taken.round(2).to_fixed(2),
            "discount_utilization": utilisation.to_fixed(2),
        },
        "recommendations": recommendations,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment(id: &str, amount: &str, discount: &str, days: i64) -> Value {
        json!({
            "payment_id": id,
            "amount": amount,
            "due_date": "2026-03-31",
            "discount_available": discount,
            "discount_days": days,
        })
    }

    #[test]
    fn a_cheap_discount_is_taken_when_the_cash_is_there() {
        let out = evaluate(&json!({
            "company_id": "c1",
            "available_cash": "100000",
            "payments": [payment("p1", "50000", "10", 10)],
        }))
        .unwrap();
        assert_eq!(out["payment_schedule"][0]["action"], "PAY_EARLY");
        assert_eq!(out["payment_schedule"][0]["savings"], "10.00");
        assert_eq!(out["savings_analysis"]["discount_taken"], "10.00");
        assert_eq!(out["savings_analysis"]["discount_utilization"], "100.00");
    }

    #[test]
    fn a_rich_discount_is_left_on_the_table_on_purpose() {
        // 10% for 10 days is far past the 20% annualised ceiling
        let out = evaluate(&json!({
            "company_id": "c1",
            "available_cash": "1000000",
            "payments": [payment("p1", "1000", "100", 10)],
        }))
        .unwrap();
        assert_eq!(out["payment_schedule"][0]["action"], "PAY_NORMAL");
        assert_eq!(out["savings_analysis"]["discount_taken"], "0.00");
        assert!(out["recommendations"][0]
            .as_str()
            .unwrap()
            .starts_with("Low discount"));
    }

    #[test]
    fn a_portfolio_of_huge_invoices_totals_exactly() {
        let out = evaluate(&json!({
            "company_id": "c1",
            "available_cash": "1",
            "payments": [
                payment("p1", "123456789012345678901234.56", "0", 10),
                payment("p2", "1", "0", 10),
            ],
        }))
        .unwrap();
        assert_eq!(
            out["savings_analysis"]["total_amount"],
            "123456789012345678901235.56"
        );
    }

    #[test]
    fn no_payments_is_no_division() {
        let out = evaluate(&json!({
            "company_id": "c1",
            "available_cash": "0",
            "payments": [],
        }))
        .unwrap();
        assert_eq!(out["savings_analysis"]["discount_utilization"], "0.00");
        assert_eq!(out["savings_analysis"]["total_payments"], 0);
    }
}
