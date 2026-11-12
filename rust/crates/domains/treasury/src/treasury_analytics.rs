//! treasury-analytics-service's snapshots, on the store.
//!
//! The KPI maths is not here: it already lives in `vimbai_treasury_fx::analytics`, held to the Python
//! answers by the seam tests. What this module adds is the record the Python service kept around
//! a run, so `/kpi/{company_id}` can answer without recalculating: the analysis is stored per
//! company, latest wins, and only the caller's Book can read it back.
//!
//! Amounts are stored as exact strings so a nine-figure treasury survives the round trip, and
//! the ratios leave as numbers, which is what FastAPI published for the Decimal fields.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, TREASURY_SNAPSHOT};

fn request_of(
    payload: &Value,
) -> Result<vimbai_treasury_fx::analytics::TreasuryAnalyticsRequest, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    Ok(vimbai_treasury_fx::analytics::TreasuryAnalyticsRequest {
        company_id: company_id.to_owned(),
        total_cash: amount_or_zero(payload, "total_cash")?,
        monthly_inflow: amount_or_zero(payload, "monthly_inflow")?,
        monthly_outflow: amount_or_zero(payload, "monthly_outflow")?,
        short_term_debt: amount_or_zero(payload, "short_term_debt")?,
        total_debt: amount_or_zero(payload, "total_debt")?,
        investments: amount_or_zero(payload, "investments")?,
        fx_exposure: amount_or_zero(payload, "fx_exposure")?,
    })
}

/// The shape the service answered with: ratios and KPI values as numbers.
fn response_of(
    company_id: &str,
    kpis: &[vimbai_treasury_fx::analytics::TreasuryKpi],
    cash_adequacy_days: &Money,
    debt_service_ratio: &Money,
    investment_yield: &Money,
    fx_risk_score: &Money,
) -> Value {
    let kpis: Vec<Value> = kpis
        .iter()
        .map(|kpi| {
            json!({
                "name": kpi.name,
                "value": wire(&kpi.value),
                "unit": kpi.unit,
                "benchmark": wire(&kpi.benchmark),
                "status": kpi.status,
                "description": kpi.description,
            })
        })
        .collect();
    json!({
        "company_id": company_id,
        "kpis": kpis,
        "cash_adequacy_days": wire(cash_adequacy_days),
        "debt_service_ratio": wire(debt_service_ratio),
        "investment_yield": wire(investment_yield),
        "fx_risk_score": wire(fx_risk_score),
    })
}

/// `/analyze`: compute the KPIs and replace the caller's previous snapshot for the company.
pub fn analyze<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let request = request_of(payload)?;
    let result = vimbai_treasury_fx::analytics::analyze_treasury(&request);
    let records = Records::new(graph, TREASURY_SNAPSHOT);

    // latest wins, matching the dict the Python service overwrote: clear this company's
    // snapshots before writing the new one.
    for previous in records.list(scope, 500)? {
        let same_company =
            previous.get("company_id").and_then(Value::as_str) == Some(request.company_id.as_str());
        if let (true, Some(id)) = (same_company, previous.get("id").and_then(Value::as_str)) {
            records.delete(scope, id)?;
        }
    }

    let stored_kpis = serde_json::to_string(
        &result
            .kpis
            .iter()
            .map(|kpi| {
                json!({
                    "name": kpi.name,
                    "value": kpi.value.to_fixed(6),
                    "unit": kpi.unit,
                    "benchmark": kpi.benchmark.to_fixed(6),
                    "status": kpi.status,
                    "description": kpi.description,
                })
            })
            .collect::<Vec<Value>>(),
    )
    .unwrap_or_else(|_| "[]".to_owned());

    records.create(
        scope,
        &json!({
            "company_id": result.company_id,
            "kpis": stored_kpis,
            "cash_adequacy_days": result.cash_adequacy_days.to_fixed(6),
            "debt_service_ratio": result.debt_service_ratio.to_fixed(6),
            "investment_yield": result.investment_yield.to_fixed(6),
            "fx_risk_score": result.fx_risk_score.to_fixed(6),
        }),
    )?;

    Ok(response_of(
        &result.company_id,
        &result.kpis,
        &result.cash_adequacy_days,
        &result.debt_service_ratio,
        &result.investment_yield,
        &result.fx_risk_score,
    ))
}

/// `/kpi/{company_id}`: the caller's stored snapshot, or the nudge the service used to answer.
pub fn kpi<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let snapshot = Records::new(graph, TREASURY_SNAPSHOT)
        .list(scope, 500)?
        .into_iter()
        .find(|record| record.get("company_id").and_then(Value::as_str) == Some(company_id));
    let Some(snapshot) = snapshot else {
        return Ok(json!({"company_id": company_id, "message": "Run /analyze first"}));
    };

    let kpis: Vec<Value> = snapshot
        .get("kpis")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default();
    Ok(json!({
        "company_id": company_id,
        "kpis": kpis,
        "cash_adequacy_days": wire(&stored_amount(&snapshot, "cash_adequacy_days")?),
        "debt_service_ratio": wire(&stored_amount(&snapshot, "debt_service_ratio")?),
        "investment_yield": wire(&stored_amount(&snapshot, "investment_yield")?),
        "fx_risk_score": wire(&stored_amount(&snapshot, "fx_risk_score")?),
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
    fn an_analysis_is_stored_exactly_and_answered_as_exact_strings() {
        // list: nothing stored yet; create: the snapshot
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "snap-1"}})]);
        let out = analyze(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "total_cash": "123456789012345.67",
                "monthly_inflow": 50000,
                "monthly_outflow": 40000,
                "short_term_debt": 10000,
                "total_debt": 100000,
                "investments": 200000,
                "fx_exposure": 5000,
            }),
        )
        .unwrap();

        // every figure on the wire is an exact decimal string, as MONEY.md requires: a JSON number
        // would be a double in the client and would drop the digits the store kept
        assert_eq!(out["company_id"], json!("c-1"));
        assert!(out["kpis"].is_array());
        assert!(out["cash_adequacy_days"].is_string());
        assert!(out["debt_service_ratio"].is_string());

        // the record keeps the money as strings, so nothing rounded on the way in
        let (cypher, params) = graph.last();
        assert!(
            cypher.contains("CREATE (x:TreasuryAnalyticsSnapshot)"),
            "{cypher}"
        );
        assert_eq!(params["props"]["book_id"], json!("book-1"));
        assert!(params["props"]["cash_adequacy_days"].is_string());
    }

    #[test]
    fn asking_before_analyzing_matches_the_old_nudge() {
        let graph = FakeGraph::returning(vec![]);
        let out = kpi(&graph, &scope(), "c-1").unwrap();
        assert_eq!(out["message"], json!("Run /analyze first"));
    }

    #[test]
    fn a_stored_snapshot_comes_back_with_its_ratios_as_numbers() {
        let graph = FakeGraph::returning(vec![json!({"x": {
            "id": "snap-1",
            "company_id": "c-1",
            "kpis": "[{\"name\": \"Cash adequacy\", \"value\": 3.5}]",
            "cash_adequacy_days": "45.500000",
            "debt_service_ratio": "1.7500000000",
            "investment_yield": "0.050000",
            "fx_risk_score": "2.000000",
        }})]);
        let out = kpi(&graph, &scope(), "c-1").unwrap();
        assert_eq!(out["cash_adequacy_days"], json!("45.500000"));
        assert_eq!(out["debt_service_ratio"], json!("1.750000"));
        assert_eq!(out["kpis"][0]["name"], json!("Cash adequacy"));
    }
}
