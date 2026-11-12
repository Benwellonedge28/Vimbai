//! Corporation tax: computation, interim provision, transfer pricing and a filing receipt.
//!
//! Four things here look like mistakes and are kept because changing them changes reported tax:
//!
//! `gross_income` is taxable income plus deductions, not gross income, and the field named
//! `taxable_income` is the amount left after deductions. The two names are inverted against what an
//! accountant would read into them.
//!
//! The rate comes from a fixed four-entry table - US_FEDERAL 21%, US_STATE 5%, UK 25%, EU 23% - and
//! anything else silently gets 25%. A jurisdiction outside that list is taxed as if it were the
//! default rather than refused.
//!
//! The provision annualises each period with `12 // period_number`, so the first period is
//! multiplied by twelve and the fourth by three, and then divides by the number of periods again.
//! That double-counts, and the total provision can exceed an annual charge on the same income.
//!
//! Deferred tax assets are computed from temporary differences and liabilities from permanent ones -
//! the labels are swapped. A permanent difference never reverses, so it should not produce a deferred
//! balance at all.
//!
//! Transaction and filing identifiers came from the clock. A pure function has neither, so a caller
//! either supplies them or does without; `next_filing_date` is the fixed 2024-04-15 the service
//! shipped, which is in the past.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn amount_sum(key: &str, payload: &Value) -> f64 {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| row.get("amount").and_then(Value::as_f64).unwrap_or(0.0))
                .sum()
        })
        .unwrap_or(0.0)
}

fn rate_for(jurisdiction: &str) -> f64 {
    match jurisdiction {
        "US_FEDERAL" => 0.21,
        "US_STATE" => 0.05,
        "UK" => 0.25,
        "EU" => 0.23,
        _ => 0.25,
    }
}

/// `POST /calculate`.
pub fn calculate(payload: &Value) -> Result<Value, String> {
    let jurisdiction = payload
        .get("jurisdiction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let declared_income = payload
        .get("taxable_income")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let deductions = amount_sum("deductions", payload);
    let taxable = (declared_income - deductions).max(0.0);
    let rate = rate_for(&jurisdiction);
    let gross_tax = taxable * rate;
    let credits = amount_sum("tax_credits", payload);
    let net_tax = (gross_tax - credits).max(0.0);
    let effective_rate = if declared_income == 0.0 {
        0.0
    } else {
        net_tax / declared_income * 100.0
    };

    let mut out = Map::new();
    for key in ["company_id", "jurisdiction", "tax_type"] {
        out.insert(key.into(), payload.get(key).cloned().unwrap_or(Value::Null));
    }
    out.insert(
        "gross_income".into(),
        Value::from(declared_income + deductions),
    );
    out.insert("total_deductions".into(), Value::from(round2(deductions)));
    out.insert("taxable_income".into(), Value::from(round2(taxable)));
    out.insert("tax_rate".into(), Value::from(rate));
    out.insert("gross_tax".into(), Value::from(round2(gross_tax)));
    out.insert("tax_credits".into(), Value::from(round2(credits)));
    out.insert("net_tax".into(), Value::from(round2(net_tax)));
    out.insert("effective_rate".into(), Value::from(round2(effective_rate)));
    Ok(Value::Object(out))
}

/// `POST /provision`.
pub fn provision(payload: &Value) -> Result<Value, String> {
    let periods = payload
        .get("interim_periods")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let annual_rate = payload
        .get("expected_annual_rate")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let incomes: Vec<f64> = payload
        .get("period_income")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();

    let mut rows = Vec::with_capacity(periods as usize);
    let mut total = 0.0;
    for index in 0..periods {
        let number = index + 1;
        let income = incomes.get(index as usize).copied().unwrap_or(0.0);
        let annualized = income * (12 / number) as f64;
        let charge = annualized * annual_rate / periods as f64;
        total += charge;
        rows.push(serde_json::json!({
            "period": number,
            "income": income,
            "annualized_income": annualized,
            "provision": round2(charge),
        }));
    }

    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    out.insert("provision_periods".into(), Value::Array(rows));
    out.insert("total_provision".into(), Value::from(round2(total)));
    out.insert(
        "deferred_tax_assets".into(),
        Value::from(round2(amount_sum("temporary_differences", payload) * 0.21)),
    );
    out.insert(
        "deferred_tax_liabilities".into(),
        Value::from(round2(amount_sum("permanent_differences", payload) * 0.21)),
    );
    out.insert(
        "effective_tax_rate".into(),
        Value::from(round2(annual_rate * 100.0)),
    );
    out.insert("reconciliation".into(), serde_json::json!([]));
    Ok(Value::Object(out))
}

/// `POST /transfer-pricing`.
pub fn transfer_pricing(payload: &Value) -> Result<Value, String> {
    let transaction = payload
        .get("transaction_amount")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let comparables = payload
        .get("comparable_data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let count = comparables.len().max(1) as f64;
    let comparable_average = comparables
        .iter()
        .map(|row| {
            row.get("price")
                .and_then(Value::as_f64)
                .unwrap_or(transaction)
        })
        .sum::<f64>()
        / count;
    let adjustment = transaction - comparable_average;

    let mut out = Map::new();
    for key in ["company_id", "transaction_id"] {
        if let Some(value) = payload.get(key) {
            out.insert(key.into(), value.clone());
        }
    }
    out.insert(
        "arm_length_amount".into(),
        Value::from(round2(comparable_average)),
    );
    out.insert(
        "adjustment_required".into(),
        Value::from(round2(adjustment)),
    );
    out.insert(
        "method_used".into(),
        Value::String("Comparable Uncontrolled Price".into()),
    );
    out.insert(
        "range_low".into(),
        Value::from(round2(comparable_average * 0.9)),
    );
    out.insert(
        "range_high".into(),
        Value::from(round2(comparable_average * 1.1)),
    );
    out.insert(
        "compliance_status".into(),
        Value::String(
            if adjustment.abs() < comparable_average * 0.05 {
                "compliant"
            } else {
                "review_required"
            }
            .into(),
        ),
    );
    Ok(Value::Object(out))
}

/// `POST /compliance`.
///
/// A receipt, not a filing: the status is always "filed", no penalty is ever computed, and the next
/// filing date is the fixed 2024-04-15. The amounts are echoed back untouched.
pub fn compliance(payload: &Value) -> Result<Value, String> {
    let mut out = Map::new();
    out.insert(
        "company_id".into(),
        payload.get("company_id").cloned().unwrap_or(Value::Null),
    );
    if let Some(id) = payload.get("filing_id") {
        out.insert("filing_id".into(), id.clone());
    }
    if let Some(date) = payload.get("filed_date") {
        out.insert("filed_date".into(), date.clone());
    }
    out.insert("status".into(), Value::String("filed".into()));
    out.insert(
        "amounts_confirmed".into(),
        payload
            .get("amounts")
            .cloned()
            .unwrap_or(serde_json::json!({})),
    );
    out.insert("penalties_interest".into(), Value::from(0.0));
    out.insert(
        "next_filing_date".into(),
        Value::String("2024-04-15".into()),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn calc() -> Value {
        json!({
            "company_id": "c-1", "jurisdiction": "UK", "tax_type": "corporation",
            "taxable_income": 1_000_000.0,
            "deductions": [{"amount": 50_000.0}, {"amount": 25_000.0}],
            "tax_credits": [{"amount": 10_000.0}],
            "period_start": "2026-01-01", "period_end": "2026-12-31"
        })
    }

    #[test]
    fn gross_income_is_declared_income_plus_deductions() {
        let out = calculate(&calc()).unwrap();
        assert_eq!(out["gross_income"], json!(1_075_000.0));
        assert_eq!(out["total_deductions"], json!(75_000.0));
        assert_eq!(out["taxable_income"], json!(925_000.0));
        assert_eq!(out["gross_tax"], json!(231_250.0));
        assert_eq!(out["net_tax"], json!(221_250.0));
    }

    #[test]
    fn the_effective_rate_divides_by_the_declared_income() {
        let out = calculate(&calc()).unwrap();
        // 221_250 / 1_000_000, not over the reduced taxable figure
        assert_eq!(out["effective_rate"], json!(22.12));
    }

    #[test]
    fn an_unlisted_jurisdiction_gets_the_default_rate() {
        let mut payload = calc();
        payload["jurisdiction"] = json!("ZW");
        let out = calculate(&payload).unwrap();
        assert_eq!(out["tax_rate"], json!(0.25));
    }

    #[test]
    fn deductions_cannot_make_taxable_income_negative() {
        let out = calculate(&json!({
            "company_id": "c-1", "jurisdiction": "US_FEDERAL", "tax_type": "corporation",
            "taxable_income": 100.0, "deductions": [{"amount": 500.0}]
        }))
        .unwrap();
        assert_eq!(out["taxable_income"], json!(0.0));
        assert_eq!(out["gross_tax"], json!(0.0));
    }

    #[test]
    fn the_provision_annualises_by_integer_division_of_twelve() {
        let out = provision(&json!({
            "company_id": "c-1", "interim_periods": 2, "expected_annual_rate": 0.25,
            "period_income": [100_000.0, 120_000.0],
            "permanent_differences": [{"amount": 40_000.0}],
            "temporary_differences": [{"amount": 80_000.0}]
        }))
        .unwrap();
        // period 1: 100k * 12 = 1.2m, times 25%, over two periods = 150k
        assert_eq!(
            out["provision_periods"][0]["annualized_income"],
            json!(1_200_000.0)
        );
        assert_eq!(out["provision_periods"][0]["provision"], json!(150_000.0));
        // period 2: 120k * 6 = 720k, times 25%, over two = 90k
        assert_eq!(out["provision_periods"][1]["provision"], json!(90_000.0));
        assert_eq!(out["total_provision"], json!(240_000.0));
    }

    #[test]
    fn deferred_assets_come_from_temporary_and_liabilities_from_permanent() {
        let out = provision(&json!({
            "company_id": "c-1", "interim_periods": 1, "expected_annual_rate": 0.25,
            "period_income": [100_000.0],
            "permanent_differences": [{"amount": 40_000.0}],
            "temporary_differences": [{"amount": 80_000.0}]
        }))
        .unwrap();
        assert_eq!(out["deferred_tax_assets"], json!(16_800.0));
        assert_eq!(out["deferred_tax_liabilities"], json!(8_400.0));
        assert_eq!(out["reconciliation"], json!([]));
    }

    #[test]
    fn transfer_pricing_flags_a_move_beyond_five_percent() {
        let out = transfer_pricing(&json!({
            "company_id": "c-1", "related_party_id": "rp-1", "transaction_type": "services",
            "transaction_amount": 1_200_000.0, "currency": "USD",
            "comparable_data": [{"price": 1_000_000.0}, {"price": 1_050_000.0}]
        }))
        .unwrap();
        // average 1_025_000, adjustment 175_000, well beyond five percent
        assert_eq!(out["arm_length_amount"], json!(1_025_000.0));
        assert_eq!(out["adjustment_required"], json!(175_000.0));
        assert_eq!(out["compliance_status"], json!("review_required"));
    }

    #[test]
    fn no_comparables_benchmarks_against_zero_rather_than_the_transaction() {
        // The fallback price only applies per comparable row, so an empty list averages to zero and
        // the whole transaction looks like an adjustment.
        let out = transfer_pricing(&json!({
            "company_id": "c-1", "transaction_amount": 500_000.0, "comparable_data": []
        }))
        .unwrap();
        assert_eq!(out["arm_length_amount"], json!(0.0));
        assert_eq!(out["adjustment_required"], json!(500_000.0));
        assert_eq!(out["compliance_status"], json!("review_required"));
    }

    #[test]
    fn the_filing_receipt_always_says_filed_with_no_penalty() {
        let out = compliance(&json!({
            "company_id": "c-1", "jurisdiction": "UK", "filing_type": "CT600",
            "period": "FY2026", "amounts": {"tax": 221_250.0}
        }))
        .unwrap();
        assert_eq!(out["status"], json!("filed"));
        assert_eq!(out["penalties_interest"], json!(0.0));
        assert_eq!(out["next_filing_date"], json!("2024-04-15"));
        assert_eq!(out["amounts_confirmed"]["tax"], json!(221_250.0));
    }
}
