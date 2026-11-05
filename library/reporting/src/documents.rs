//! Document totals: invoices, quotes, receipts and statements.
//!
//! The service did no rounding until it printed. Each line's total and tax are rounded for display,
//! but the subtotal, the tax total and the grand total are summed from the unrounded line figures
//! and rounded once at the end. That difference is visible on long documents - a hundred lines each
//! rounded first can drift a cent or two from the true total - so the order is kept.
//!
//! A missing quantity is one, a missing price or rate is zero, exactly as the `.get()` calls defaulted.
//!
//! The document id is not minted here. Python defaulted it to a fresh uuid4, which a pure function
//! cannot reproduce; the caller supplies one when it wants one, and may leave it out.

use serde_json::{Map, Value};

fn round2(value: f64) -> f64 {
    vimbai_common::numbers::round_dp(value, 2)
}

fn number(value: Option<&Value>, fallback: f64) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(fallback),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(fallback),
        _ => fallback,
    }
}

fn string_of(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(Value::Number(n)) => Value::String(n.to_string()),
        Some(other) => other.clone(),
        None => Value::String(String::new()),
    }
}

/// The totals of one document, from its line items.
fn totals(payload: &Value) -> (f64, f64, Vec<Value>) {
    let mut subtotal = 0.0;
    let mut tax_total = 0.0;
    let mut lines = Vec::new();

    for item in payload
        .get("line_items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let quantity = number(item.get("quantity"), 1.0);
        let unit_price = number(item.get("unit_price"), 0.0);
        let tax_rate = number(item.get("tax_rate"), 0.0);
        let line_total = quantity * unit_price;
        let line_tax = line_total * tax_rate;
        subtotal += line_total;
        tax_total += line_tax;
        lines.push(serde_json::json!({
            "description": string_of(item.get("description")),
            "quantity": quantity,
            "unit_price": unit_price,
            "line_total": round2(line_total),
            "tax": round2(line_tax),
        }));
    }
    (subtotal, tax_total, lines)
}

/// `POST /generate` of the business documents service.
pub fn generate(payload: &Value) -> Result<Value, String> {
    let (subtotal, tax_total, lines) = totals(payload);
    let mut out = Map::new();
    if let Some(id) = payload.get("id") {
        out.insert("id".into(), id.clone());
    }
    for field in [
        "company_id",
        "document_type",
        "reference_number",
        "date",
        "company_name",
        "recipient",
        "notes",
        "payment_terms",
    ] {
        out.insert(
            field.into(),
            payload
                .get(field)
                .cloned()
                .unwrap_or(Value::String(String::new())),
        );
    }
    out.insert("subtotal".into(), Value::from(round2(subtotal)));
    out.insert("tax_total".into(), Value::from(round2(tax_total)));
    out.insert(
        "grand_total".into(),
        Value::from(round2(subtotal + tax_total)),
    );
    out.insert("line_items".into(), Value::Array(lines));
    Ok(Value::Object(out))
}

/// `POST /generate/batch`: one document per entry, each stamped with the batch's company.
pub fn generate_batch(payload: &Value) -> Result<Value, String> {
    let documents = payload
        .get("documents")
        .and_then(Value::as_array)
        .ok_or_else(|| "documents is required".to_string())?;
    let company_id = payload.get("company_id").cloned().unwrap_or(Value::Null);

    let mut results = Vec::with_capacity(documents.len());
    for document in documents {
        let mut copy = document.clone();
        if let Some(object) = copy.as_object_mut() {
            object.insert("company_id".into(), company_id.clone());
        }
        results.push(generate(&copy)?);
    }
    Ok(Value::Array(results))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tax_is_charged_per_line_and_summed_unrounded() {
        let out = generate(&json!({
            "company_id": "c-1",
            "line_items": [
                {"description": "A", "quantity": 2, "unit_price": 750.0, "tax_rate": 0.15},
                {"description": "B", "quantity": 1, "unit_price": 1200.5, "tax_rate": 0.0}
            ]
        }))
        .unwrap();
        assert_eq!(out["subtotal"], json!(2700.5));
        assert_eq!(out["tax_total"], json!(225.0));
        assert_eq!(out["grand_total"], json!(2925.5));
        assert_eq!(out["line_items"][0]["line_total"], json!(1500.0));
    }

    #[test]
    fn a_missing_quantity_is_one_and_a_missing_price_is_nothing() {
        let out =
            generate(&json!({"company_id": "c-1", "line_items": [{"description": "Free sample"}]}))
                .unwrap();
        assert_eq!(out["line_items"][0]["quantity"], json!(1.0));
        assert_eq!(out["line_items"][0]["line_total"], json!(0.0));
        assert_eq!(out["grand_total"], json!(0.0));
    }

    #[test]
    fn a_document_with_no_id_expects_the_caller_to_supply_one() {
        let out = generate(&json!({"company_id": "c-1", "line_items": []})).unwrap();
        assert!(out.get("id").is_none());
        let named = generate(&json!({"id": "doc-1", "company_id": "c-1"})).unwrap();
        assert_eq!(named["id"], json!("doc-1"));
    }

    #[test]
    fn a_batch_stamps_every_document_with_its_company() {
        let out = generate_batch(&json!({
            "company_id": "c-9",
            "documents": [{"company_id": "wrong", "line_items": []}, {"line_items": []}]
        }))
        .unwrap();
        assert_eq!(out[0]["company_id"], json!("c-9"));
        assert_eq!(out[1]["company_id"], json!("c-9"));
    }
}
