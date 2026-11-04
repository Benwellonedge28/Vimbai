//! IFRS 8 operating segments, from segment-reporting-service.
//!
//! A segment is reportable once it reaches a tenth of revenue, of the absolute profit, or
//! of assets. Money rounds to two places and the revenue share to one, as the service did.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

fn number(payload: &Value, key: &str) -> Result<f64, String> {
    match payload.get(key) {
        Some(Value::Number(value)) => value
            .as_f64()
            .ok_or_else(|| format!("{key} is not a number")),
        Some(Value::String(text)) => text
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{key} is not a number")),
        Some(other) => Err(format!("{key} is {other}, expected a number")),
        None => Err(format!("{key} is required")),
    }
}

fn optional(payload: &Value, key: &str) -> Result<f64, String> {
    if payload.get(key).is_none() || payload.get(key) == Some(&Value::Null) {
        return Ok(0.0);
    }
    number(payload, key)
}

fn rounded(value: f64, places: usize) -> f64 {
    format!("{value:.places$}").parse::<f64>().unwrap_or(value)
}

fn share(part: f64, whole: f64) -> f64 {
    if whole == 0.0 {
        0.0
    } else {
        part / whole * 100.0
    }
}

/// `/report`
pub fn report_from_json(payload: &Value) -> Result<Value, String> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or("company_id is required")?
        .to_string();
    let fiscal_year = payload
        .get("fiscal_year")
        .and_then(Value::as_i64)
        .ok_or("fiscal_year is required")?;
    let segments = payload
        .get("segments")
        .and_then(Value::as_array)
        .ok_or("segments must be a list")?;
    let reconciliation_revenue = optional(payload, "reconciliation_revenue")?;
    let reconciliation_expenses = optional(payload, "reconciliation_expenses")?;

    let total_revenue: f64 = segments
        .iter()
        .map(|segment| number(segment, "revenue"))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    let total_profit: f64 = segments
        .iter()
        .map(|segment| number(segment, "profit_or_loss"))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    let total_assets: f64 = segments
        .iter()
        .map(|segment| number(segment, "assets"))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    let total_expenses: f64 = segments
        .iter()
        .map(|segment| number(segment, "expenses"))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();

    let mut reportable = 0;
    let mut detail = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        let revenue = number(segment, "revenue")?;
        let expenses = number(segment, "expenses")?;
        let profit = number(segment, "profit_or_loss")?;
        let assets = number(segment, "assets")?;
        let liabilities = optional(segment, "liabilities")?;

        let revenue_pct = share(revenue, total_revenue);
        let profit_pct = if total_profit == 0.0 {
            0.0
        } else {
            profit.abs() / total_profit.abs() * 100.0
        };
        let asset_pct = share(assets, total_assets);
        let is_reportable = revenue_pct >= 10.0 || profit_pct >= 10.0 || asset_pct >= 10.0;
        if is_reportable {
            reportable += 1;
        }

        let segment_id = segment
            .get("segment_id")
            .and_then(Value::as_str)
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("segment-{}", index + 1));

        detail.push(json!({
            "segment_id": segment_id,
            "name": segment.get("name").and_then(Value::as_str).unwrap_or(""),
            "type": segment.get("segment_type").and_then(Value::as_str).unwrap_or(""),
            "revenue": rounded(revenue, 2),
            "expenses": rounded(expenses, 2),
            "profit_or_loss": rounded(profit, 2),
            "assets": rounded(assets, 2),
            "liabilities": rounded(liabilities, 2),
            "revenue_pct": rounded(revenue_pct, 1),
            "is_reportable": is_reportable,
        }));
    }

    Ok(json!({
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "reportable_segments": reportable,
        "total_revenue": rounded(total_revenue, 2),
        "total_profit": rounded(total_profit, 2),
        "total_assets": rounded(total_assets, 2),
        "segments": detail,
        "reconciliation": {
            "segments_revenue": rounded(total_revenue, 2),
            "consolidated_revenue": rounded(total_revenue + reconciliation_revenue, 2),
            "reconciliation_difference": rounded(reconciliation_revenue, 2),
            "segments_expenses": rounded(total_expenses, 2),
            "consolidated_expenses": rounded(total_expenses + reconciliation_expenses, 2),
        },
        "disclosure_notes": [
            format!("{reportable} reportable segments identified under IFRS 8"),
            "Segment identification based on internal management reporting",
            "Geographic and business segment information disclosed separately",
            "Inter-segment transactions eliminated in reconciliation",
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Value {
        report_from_json(&json!({
            "company_id": "c-1",
            "fiscal_year": 2026,
            "segments": [
                {"name": "Retail", "segment_type": "business", "revenue": 600000.0, "expenses": 400000.0, "profit_or_loss": 200000.0, "assets": 900000.0},
                {"name": "Wholesale", "segment_type": "business", "revenue": 300000.0, "expenses": 260000.0, "profit_or_loss": 40000.0, "assets": 500000.0},
                {"name": "Other", "segment_type": "geographical", "revenue": 50000.0, "expenses": 45000.0, "profit_or_loss": 5000.0, "assets": 60000.0}
            ],
            "reconciliation_revenue": 10000.0
        }))
        .unwrap()
    }

    #[test]
    fn a_segment_over_a_tenth_of_anything_is_reportable() {
        let out = report();
        // Retail and Wholesale clear the revenue and asset bars; Other clears none of them.
        assert_eq!(out["reportable_segments"], json!(2));
    }

    #[test]
    fn totals_are_the_sum_of_the_segments() {
        let out = report();
        assert_eq!(out["total_revenue"], json!(950000.0));
        assert_eq!(out["total_profit"], json!(245000.0));
        assert_eq!(out["total_assets"], json!(1460000.0));
    }

    #[test]
    fn the_reconciliation_adds_the_unallocated_amount() {
        let out = report();
        assert_eq!(out["reconciliation"]["segments_revenue"], json!(950000.0));
        assert_eq!(
            out["reconciliation"]["consolidated_revenue"],
            json!(960000.0)
        );
        assert_eq!(
            out["reconciliation"]["reconciliation_difference"],
            json!(10000.0)
        );
    }

    #[test]
    fn an_empty_report_does_not_divide_by_zero() {
        let out = report_from_json(&json!({
            "company_id": "c-2",
            "fiscal_year": 2026,
            "segments": []
        }))
        .unwrap();
        assert_eq!(out["reportable_segments"], json!(0));
        assert_eq!(out["total_revenue"], json!(0.0));
    }
}
