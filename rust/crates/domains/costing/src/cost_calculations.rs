//! The cost calculations behind process-costing-service and product-costing-service, on the store.
//!
//! A calculation is a set of cost components attributed to one product or process in one period.
//! Its totals are pure arithmetic over those components, and the arithmetic is done in `Money`
//! rather than `f64`: a breakdown with long component amounts used to report a total that drifted
//! in the last digits, and the unit cost then divided that drift by the quantity.
//!
//! The totals are stored with the record because every other endpoint reports them, but the
//! components are what the record means: the breakdown is recomputed from them, not from the total.
//!
//! There is one implementation because there is one record. The two Python services were generated
//! from the same template: identical models, identical endpoints, identical `CostCalculation` label,
//! and both list by company alone, so each already returned the other's records. Porting them
//! separately in Rust would have made that accident permanent.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, COST_CALCULATION};

/// One line of a submission: what the cost was called, what it came to, and which bucket it is in.
struct Component {
    name: String,
    amount: Money,
    cost_type: String,
}

fn components(payload: &Value) -> Result<Vec<Component>, StoreError> {
    let Some(rows) = payload.get("components") else {
        return Ok(Vec::new());
    };
    if rows.is_null() {
        return Ok(Vec::new());
    }
    let rows = rows
        .as_array()
        .ok_or_else(|| StoreError::Invalid("components must be a list".into()))?;
    rows.iter()
        .map(|row| {
            Ok(Component {
                name: row
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                amount: amount(row, "amount")?,
                cost_type: row
                    .get("cost_type")
                    .and_then(Value::as_str)
                    .unwrap_or("direct")
                    .to_owned(),
            })
        })
        .collect()
}

/// The stored components, read back exactly.
fn stored_components(row: &Value) -> Result<Vec<Component>, StoreError> {
    let Some(rows) = row.get("components").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    rows.iter()
        .map(|line| {
            Ok(Component {
                name: line
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                amount: stored_amount(line, "amount")?,
                cost_type: line
                    .get("cost_type")
                    .and_then(Value::as_str)
                    .unwrap_or("direct")
                    .to_owned(),
            })
        })
        .collect()
}

fn components_wire(parts: &[Component]) -> Value {
    Value::Array(
        parts
            .iter()
            .map(|part| {
                json!({
                    "name": part.name,
                    "amount": wire(&part.amount),
                    "cost_type": part.cost_type,
                })
            })
            .collect(),
    )
}

fn calculation_json(row: &Value) -> Result<Value, StoreError> {
    let parts = stored_components(row)?;
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(Value::Null),
        "company_id": row.get("company_id").cloned().unwrap_or(Value::Null),
        "product_or_process": row.get("product_or_process").cloned().unwrap_or(Value::Null),
        "period": row.get("period").cloned().unwrap_or_else(|| json!("")),
        "components": components_wire(&parts),
        "total_cost": wire(&stored_amount(row, "total_cost")?),
        "unit_cost": wire(&stored_amount(row, "unit_cost")?),
        "quantity": row.get("quantity").cloned().unwrap_or_else(|| json!(1)),
        "notes": row.get("notes").cloned().unwrap_or_else(|| json!("")),
        "created_at": row.get("created_at").cloned().unwrap_or(Value::Null),
    }))
}

fn company_calculations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let rows = Records::new(graph, COST_CALCULATION).list(scope, 500)?;
    Ok(rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect())
}

/// `/calculate`: store one calculation, stamping it with the caller's Book.
///
/// The quantity floor is deliberate and matches what the Python service did: a calculation of zero
/// units still has a unit cost, so the total is divided by at least one rather than refusing the
/// submission.
pub fn create_calculation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let product = payload
        .get("product_or_process")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("product_or_process is required".into()))?;
    let period = payload
        .get("period")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let notes = payload
        .get("notes")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let quantity = payload
        .get("quantity")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1);
    let parts = components(payload)?;

    let total = Money::sum(parts.iter().map(|part| &part.amount));
    let unit = total.over(&Money::from_i64(quantity));

    let record = json!({
        "company_id": company_id,
        "product_or_process": product,
        "period": period,
        "components": components_wire(&parts),
        "total_cost": total.to_fixed(6),
        "unit_cost": unit.to_fixed(6),
        "quantity": quantity,
        "notes": notes,
    });
    let saved = Records::new(graph, COST_CALCULATION).create(scope, &record)?;
    calculation_json(&saved)
}

/// `/calculations/{company_id}`: the caller's calculations, optionally filtered by product.
pub fn calculations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    product: Option<&str>,
) -> Result<Value, StoreError> {
    let needle = product.unwrap_or_default().to_lowercase();
    let mut rows = Vec::new();
    for row in company_calculations(graph, scope, company_id)? {
        if !needle.is_empty() {
            let name = row
                .get("product_or_process")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_lowercase();
            if !name.contains(&needle) {
                continue;
            }
        }
        rows.push(calculation_json(&row)?);
    }
    Ok(json!({
        "company_id": company_id,
        "calculations": rows,
        "total": rows_len(&rows),
    }))
}

fn rows_len(rows: &[Value]) -> usize {
    rows.len()
}

/// `/breakdown/{company_id}/{calc_id}`: the components grouped by cost type.
pub fn breakdown<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    calc_id: &str,
) -> Result<Value, StoreError> {
    let row = company_calculations(graph, scope, company_id)?
        .into_iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(calc_id))
        .ok_or(StoreError::NotFound)?;
    let parts = stored_components(&row)?;

    let mut buckets: Vec<(String, Money)> = Vec::new();
    for part in &parts {
        match buckets.iter_mut().find(|(name, _)| name == &part.cost_type) {
            Some((_, running)) => *running = running.plus(&part.amount),
            None => buckets.push((part.cost_type.clone(), part.amount.clone())),
        }
    }
    let mut breakdown = serde_json::Map::new();
    for (bucket, total) in buckets {
        breakdown.insert(bucket, wire(&total));
    }

    Ok(json!({
        "calc_id": calc_id,
        "total": wire(&stored_amount(&row, "total_cost")?),
        "unit_cost": wire(&stored_amount(&row, "unit_cost")?),
        "breakdown": Value::Object(breakdown),
        "components": components_wire(&parts),
    }))
}

/// `/summary/{company_id}`: how many calculations, what they came to, and the average unit cost.
pub fn summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let rows = company_calculations(graph, scope, company_id)?;
    if rows.is_empty() {
        return Ok(json!({
            "company_id": company_id,
            "total_calculations": 0,
            "total_cost": 0.0,
            "avg_unit_cost": 0.0,
        }));
    }

    let mut totals = Vec::with_capacity(rows.len());
    let mut units = Vec::with_capacity(rows.len());
    for row in &rows {
        totals.push(stored_amount(row, "total_cost")?);
        units.push(stored_amount(row, "unit_cost")?);
    }
    let total = Money::sum(totals.iter());
    let average_unit = Money::sum(units.iter()).over(&Money::from_i64(rows.len() as i64));

    Ok(json!({
        "company_id": company_id,
        "total_calculations": rows.len(),
        "total_cost": wire(&total),
        "avg_unit_cost": wire(&average_unit),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_calculation_totals_its_components_exactly() {
        // three components whose float sum drifts: 0.1 + 0.2 is the classic case
        let graph = vimbai_store::fake::FakeGraph::returning(vec![json!({"x": {
            "id": "calc-1",
            "company_id": "c-1",
            "product_or_process": "Widgets",
            "components": [],
            "total_cost": "0",
            "unit_cost": "0",
            "quantity": 1
        }})]);
        create_calculation(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "product_or_process": "Widgets",
                "quantity": 3,
                "components": [
                    {"name": "resin", "amount": 0.1, "cost_type": "direct_materials"},
                    {"name": "labour", "amount": 0.2, "cost_type": "direct_labor"},
                    {"name": "power", "amount": "0.3", "cost_type": "overhead"},
                ]
            }),
        )
        .unwrap();

        let (cypher, params) = graph.last();
        assert!(cypher.contains("CREATE (x:CostCalculation)"), "{cypher}");
        assert_eq!(params["props"]["total_cost"], json!("0.600000"));
        assert_eq!(params["props"]["unit_cost"], json!("0.200000"));
        assert_eq!(params["props"]["quantity"], json!(3));
    }

    #[test]
    fn a_long_component_is_not_rounded_on_its_way_through() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![json!({"x": {
            "id": "calc-1",
            "total_cost": "0",
            "unit_cost": "0",
            "quantity": 1,
            "components": []
        }})]);
        create_calculation(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "product_or_process": "Turbine",
                "components": [{"name": "casting", "amount": "12345678901234567890.55"}]
            }),
        )
        .unwrap();
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["total_cost"],
            json!("12345678901234567890.550000")
        );
    }

    #[test]
    fn a_zero_quantity_still_has_a_unit_cost() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![json!({"x": {
            "id": "calc-1", "total_cost": "0", "unit_cost": "0", "quantity": 1, "components": []
        }})]);
        create_calculation(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "product_or_process": "Setup",
                "quantity": 0,
                "components": [{"name": "setup", "amount": 100}]
            }),
        )
        .unwrap();
        let (_, params) = graph.last();
        assert_eq!(params["props"]["unit_cost"], json!("100.000000"));
    }

    #[test]
    fn a_calculation_needs_a_product_and_a_company() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![]);
        let err = create_calculation(&graph, &scope(), &json!({"company_id": "c-1"})).unwrap_err();
        assert!(err.to_string().contains("product_or_process"), "{err}");
        assert_eq!(graph.statements(), 0, "nothing should be written");
    }

    #[test]
    fn a_breakdown_groups_the_components_by_cost_type() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![json!({"x": {
            "id": "calc-1",
            "company_id": "c-1",
            "product_or_process": "Widgets",
            "total_cost": "600",
            "unit_cost": "200",
            "quantity": 3,
            "components": [
                {"name": "resin", "amount": 100, "cost_type": "direct_materials"},
                {"name": "paint", "amount": 150, "cost_type": "direct_materials"},
                {"name": "labour", "amount": 350, "cost_type": "direct_labor"}
            ]
        }})]);
        let out = breakdown(&graph, &scope(), "c-1", "calc-1").unwrap();
        assert_eq!(out["breakdown"]["direct_materials"], json!("250.000000"));
        assert_eq!(out["breakdown"]["direct_labor"], json!("350.000000"));
        assert_eq!(out["total"], json!("600.000000"));
    }

    #[test]
    fn a_breakdown_of_someone_elses_calculation_is_not_found() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![json!({"x": {
            "id": "calc-2", "company_id": "c-1", "components": []
        }})]);
        let err = breakdown(&graph, &scope(), "c-1", "calc-1").unwrap_err();
        assert_eq!(err, StoreError::NotFound);
    }

    #[test]
    fn a_summary_with_nothing_in_it_reports_zeros_not_an_error() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![]);
        let out = summary(&graph, &scope(), "c-1").unwrap();
        assert_eq!(out["total_calculations"], json!(0));
        assert_eq!(out["total_cost"], json!(0.0));
        assert_eq!(out["avg_unit_cost"], json!(0.0));
    }

    #[test]
    fn a_summary_sums_the_companys_calculations_only() {
        let graph = vimbai_store::fake::FakeGraph::returning(vec![
            json!({"x": {"id": "a", "company_id": "c-1", "total_cost": "600", "unit_cost": "200", "components": []}}),
            json!({"x": {"id": "b", "company_id": "c-1", "total_cost": "400", "unit_cost": "100", "components": []}}),
            json!({"x": {"id": "c", "company_id": "c-2", "total_cost": "9999", "unit_cost": "9999", "components": []}}),
        ]);
        let out = summary(&graph, &scope(), "c-1").unwrap();
        assert_eq!(out["total_calculations"], json!(2));
        assert_eq!(out["total_cost"], json!("1000.000000"));
        assert_eq!(out["avg_unit_cost"], json!("150.000000"));
    }
}
