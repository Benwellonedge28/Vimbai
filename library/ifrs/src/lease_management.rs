//! Lease measurement as the lease management service published it.
//!
//! The service discounted each lease's payments one period at a time, treated the present value as
//! both the liability and the right-of-use asset, and spread the total liability evenly over five
//! years for the maturity schedule. Two of those are deliberate simplifications rather than IFRS 16:
//! an even five-year spread ignores the actual payment dates, and the right-of-use asset equals the
//! liability rather than the liability plus initial direct costs. They are kept because callers hold
//! numbers produced this way, and IFRS 16 in full is available separately as `calculate_ifrs16`.
//!
//! The payment endpoint is a placeholder in the original: principal at 70% of the payment, interest
//! at 30%, and a fixed remaining balance of 100000 regardless of the lease. Nothing here improves on
//! that, because improving it would change figures a client has already recorded.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Present value of a payment stream, discounted from period one.
fn present_value(payments: &[f64], rate: f64) -> f64 {
    payments
        .iter()
        .enumerate()
        .map(|(index, payment)| payment / (1.0 + rate).powi(index as i32 + 1))
        .sum()
}

/// `POST /calculate` of the lease management service.
pub fn calculate(payload: &Value) -> Result<Value, String> {
    let leases = payload
        .get("leases")
        .and_then(Value::as_array)
        .ok_or_else(|| "leases is required".to_string())?;

    let mut total_liability = 0.0;
    let mut details = Vec::with_capacity(leases.len());

    for lease in leases {
        let payments: Vec<f64> = lease
            .get("payments")
            .and_then(Value::as_array)
            .map(|values| values.iter().map(|v| number(Some(v))).collect())
            .unwrap_or_default();
        let rate = number(lease.get("discount_rate"));
        let pv = present_value(&payments, rate);
        total_liability += pv;
        details.push(serde_json::json!({
            "lease_id": lease.get("lease_id").cloned().unwrap_or(Value::Null),
            "liability": round2(pv),
            "asset": round2(pv),
        }));
    }

    let fifth = round2(total_liability * 0.2);
    let maturity: Vec<Value> = (1..=5)
        .map(|year| serde_json::json!({"year": year, "amount": fifth}))
        .collect();

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "total_lease_liability".into(),
        Value::from(round2(total_liability)),
    );
    out.insert(
        "total_right_of_use_asset".into(),
        Value::from(round2(total_liability)),
    );
    out.insert("lease_details".into(), Value::Array(details));
    out.insert("maturity_schedule".into(), Value::Array(maturity));
    Ok(Value::Object(out))
}

/// `POST /payment` of the lease management service, placeholder split and all.
pub fn process_payment(payload: &Value) -> Result<Value, String> {
    let amount = number(payload.get("payment_amount"));
    let principal = round2(amount * 0.7);
    let mut out = Map::new();
    out.insert(
        "lease_id".into(),
        payload.get("lease_id").cloned().unwrap_or(Value::Null),
    );
    out.insert("principal_paid".into(), Value::from(principal));
    out.insert("interest_paid".into(), Value::from(round2(amount * 0.3)));
    out.insert("remaining_balance".into(), Value::from(100000.0));
    out.insert("liability_reduction".into(), Value::from(principal));
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_single_payment_for_one_period_is_discounted_once() {
        let out = calculate(&json!({
            "company_id": "c-1",
            "leases": [{"lease_id": "L1", "payments": [110.0], "discount_rate": 0.1}]
        }))
        .unwrap();
        assert_eq!(out["lease_details"][0]["liability"], json!(100.0));
    }

    #[test]
    fn the_right_of_use_asset_mirrors_the_liability() {
        let out = calculate(&json!({
            "company_id": "c-1",
            "leases": [{"lease_id": "L1", "payments": [110.0], "discount_rate": 0.1}]
        }))
        .unwrap();
        assert_eq!(
            out["total_right_of_use_asset"],
            out["total_lease_liability"]
        );
    }

    #[test]
    fn a_lease_with_no_payments_measures_nothing() {
        let out = calculate(&json!({"company_id": "c-1", "leases": [{"lease_id": "L1", "payments": [], "discount_rate": 0.1}]}))
            .unwrap();
        assert_eq!(out["total_lease_liability"], json!(0.0));
    }

    #[test]
    fn the_payment_split_is_the_placeholder_ninety_thirty() {
        let out = process_payment(&json!({"lease_id": "L1", "payment_amount": 1000.0})).unwrap();
        assert_eq!(out["principal_paid"], json!(700.0));
        assert_eq!(out["interest_paid"], json!(300.0));
        assert_eq!(out["remaining_balance"], json!(100000.0));
    }
}
