//! job-costing-service's records, on the store.
//!
//! A job carries a contract value and the costs charged to it. Adding a cost both keeps the entry
//! and moves the job's running total, which is why this module is more than a wrapper around the
//! store: the two writes have to agree, and the money has to stay exact while it is being added
//! up. The margin sweeps are computed here with `Money`, so a job with a long contract value does
//! not report a margin that wanders in the last digits.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, JOB, JOB_COST_ENTRY};

fn company_jobs<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let rows = Records::new(graph, JOB).list(scope, 500)?;
    Ok(rows
        .into_iter()
        .filter(|job| job.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect())
}

/// `/jobs`: open a job, stamped with the caller's Book.
pub fn create_job<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let contract = amount(payload, "contract_value")?;
    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("open");

    let mut record = payload.clone();
    record["company_id"] = json!(company_id);
    record["status"] = json!(status);
    record["contract_value"] = json!(contract.to_fixed(6));
    record["total_cost"] = json!("0.000000");
    record["gross_profit"] = json!(contract.to_fixed(6));
    record["gross_margin"] = json!(if contract.is_zero() {
        0.0
    } else {
        contract.ratio_pct(&contract, 6).to_f64_lossy()
    });
    Records::new(graph, JOB).create(scope, &record)
}

/// `/jobs/{company_id}`: the caller's jobs for a company, optionally by status.
pub fn jobs<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    status: Option<&str>,
) -> Result<Value, StoreError> {
    let jobs: Vec<Value> = company_jobs(graph, scope, company_id)?
        .into_iter()
        .filter(|job| match status {
            None => true,
            Some(wanted) => job.get("status").and_then(Value::as_str) == Some(wanted),
        })
        .collect();
    Ok(json!({
        "company_id": company_id,
        "total": jobs.len(),
        "jobs": jobs,
    }))
}

/// `/jobs/{job_id}/costs`: charge a cost to a job and move its running total.
///
/// A job that is not the caller's is a miss, so a cost can never be charged to someone else's job
/// by guessing its id.
pub fn add_cost<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    job_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let job = Records::new(graph, JOB).get(scope, job_id)?;
    let charge = amount(payload, "amount")?;
    let mut entry = payload.clone();
    entry["job_id"] = json!(job_id);
    entry["amount"] = json!(charge.to_fixed(6));
    if entry.get("cost_type").is_none() {
        entry["cost_type"] = json!("other");
    }
    let saved_entry = Records::new(graph, JOB_COST_ENTRY).create(scope, &entry)?;

    let running = stored_amount(&job, "total_cost")?.plus(&charge);
    let contract = stored_amount(&job, "contract_value")?;
    let profit = contract.minus(&running);
    let margin = if contract.is_zero() {
        0.0
    } else {
        profit.ratio_pct(&contract, 6).to_f64_lossy()
    };
    let moved = Records::new(graph, JOB).update(
        scope,
        job_id,
        &json!({
            "total_cost": running.to_fixed(6),
            "gross_profit": profit.to_fixed(6),
            "gross_margin": margin,
        }),
    )?;

    Ok(json!({
        "job": moved,
        "entry": saved_entry,
        "total_cost": wire(&running),
        "gross_profit": wire(&profit),
        "gross_margin": margin,
    }))
}

/// `/jobs/{company_id}/profitability`: the company's jobs, summed exactly.
pub fn profitability<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let jobs = company_jobs(graph, scope, company_id)?;
    let mut contracts = Vec::with_capacity(jobs.len());
    let mut costs = Vec::with_capacity(jobs.len());
    let mut profits = Vec::with_capacity(jobs.len());
    let mut margins = Vec::with_capacity(jobs.len());
    let completed = jobs
        .iter()
        .filter(|job| {
            matches!(
                job.get("status").and_then(Value::as_str),
                Some("completed") | Some("closed")
            )
        })
        .count();

    for job in &jobs {
        contracts.push(stored_amount(job, "contract_value")?);
        costs.push(stored_amount(job, "total_cost")?);
        profits.push(stored_amount(job, "gross_profit")?);
        margins.push(
            job.get("gross_margin")
                .and_then(Value::as_f64)
                .unwrap_or_default(),
        );
    }

    let count = jobs.len();
    let average_margin = if count == 0 {
        0.0
    } else {
        margins.iter().sum::<f64>() / count as f64
    };
    Ok(json!({
        "company_id": company_id,
        "total_jobs": count,
        "completed": completed,
        "total_contract_value": wire(&Money::sum(&contracts)),
        "total_cost": wire(&Money::sum(&costs)),
        "total_profit": wire(&Money::sum(&profits)),
        "avg_margin": average_margin,
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
    fn a_new_job_starts_with_its_contract_as_profit() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "job-1"}})]);
        create_job(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "contract_value": 1000000, "status": "open"}),
        )
        .unwrap();
        let (cypher, params) = graph.last();
        assert!(cypher.contains("CREATE (x:Job)"), "{cypher}");
        assert_eq!(params["props"]["contract_value"], json!("1000000.000000"));
        assert_eq!(params["props"]["total_cost"], json!("0.000000"));
        assert_eq!(params["props"]["gross_profit"], json!("1000000.000000"));
    }

    #[test]
    fn charging_a_cost_moves_the_jobs_running_total() {
        // first statement: read the job; second: create the entry; third: move the totals
        let graph = FakeGraph::returning(vec![json!({"x": {
            "id": "job-1",
            "contract_value": "1000000",
            "total_cost": "250000",
            "book_id": "book-1"
        }})]);
        let out = add_cost(
            &graph,
            &scope(),
            "job-1",
            &json!({"cost_type": "material", "amount": 50000}),
        )
        .unwrap();
        assert_eq!(out["total_cost"], json!("300000.000000"));
        assert_eq!(out["gross_profit"], json!("700000.000000"));
        assert_eq!(out["gross_margin"], json!(70.0));
    }

    #[test]
    fn a_cost_cannot_be_charged_to_a_job_outside_the_book() {
        let graph = FakeGraph::returning(vec![]);
        let err = add_cost(&graph, &scope(), "job-other", &json!({"amount": 10})).unwrap_err();
        assert_eq!(err, StoreError::NotFound);
        assert_eq!(graph.statements(), 1, "nothing should be written");
    }

    #[test]
    fn profitability_sums_the_companys_jobs() {
        let graph = FakeGraph::returning(vec![
            json!({"x": {"company_id": "c-1", "status": "completed", "contract_value": "500000", "total_cost": "420000", "gross_profit": "80000", "gross_margin": 16.0}}),
            json!({"x": {"company_id": "c-1", "status": "open", "contract_value": "250000", "total_cost": "90000", "gross_profit": "160000", "gross_margin": 64.0}}),
            json!({"x": {"company_id": "c-2", "status": "open", "contract_value": "999999", "total_cost": "1", "gross_profit": "1", "gross_margin": 1.0}}),
        ]);
        let out = profitability(&graph, &scope(), "c-1").unwrap();
        assert_eq!(out["total_jobs"], json!(2));
        assert_eq!(out["completed"], json!(1));
        assert_eq!(out["total_contract_value"], json!("750000.000000"));
        assert_eq!(out["total_cost"], json!("510000.000000"));
        assert_eq!(out["total_profit"], json!("240000.000000"));
        assert_eq!(out["avg_margin"], json!(40.0));
    }
}
