//! insurance-claims-service's records, on the store.
//!
//! Filing a claim is a plain write; processing one is not. The settlement is
//! `min(claim, coverage_limit) - deductible`, floored at zero, and the decision that follows
//! (approved when anything is covered, denied when nothing is) is written back onto the claim in
//! the same call. The arithmetic runs on `Money` so a seven-figure claim does not lose cents
//! while it is being capped and deducted.

#![forbid(unsafe_code)]

use serde_json::{json, Value};

use vimbai_common::Money;

use vimbai_store::amounts::{amount, amount_or_zero, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, INSURANCE_CLAIM};

/// The claim as the caller sees it, whether it just arrived or came back from a list.
fn claim_of(record: Value) -> Value {
    record
}

/// `/file`: file a claim against the caller's Book.
pub fn file_claim<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let policy_number = payload
        .get("policy_number")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("policy_number is required".into()))?;
    let claim_amount = amount(payload, "claim_amount")?;
    let deductible = amount_or_zero(payload, "deductible")?;
    let coverage_limit = amount_or_zero(payload, "coverage_limit")?;

    // The Python model kept the supporting documents as a JSON string on the node, so existing
    // claims stay readable; the same shape is written here.
    let docs = payload
        .get("supporting_docs")
        .cloned()
        .unwrap_or_else(|| json!([]));

    let record = json!({
        "company_id": company_id,
        "policy_number": policy_number,
        "claim_type": payload.get("claim_type").and_then(Value::as_str).unwrap_or(""),
        "incident_date": payload.get("incident_date").and_then(Value::as_str).unwrap_or(""),
        "claim_date": payload.get("claim_date").and_then(Value::as_str).unwrap_or(""),
        "claim_amount": claim_amount.to_fixed(6),
        "deductible": deductible.to_fixed(6),
        "coverage_limit": coverage_limit.to_fixed(6),
        "description": payload.get("description").and_then(Value::as_str).unwrap_or(""),
        "supporting_docs_json": docs.to_string(),
        "status": "filed",
    });
    Records::new(graph, INSURANCE_CLAIM).create(scope, &record)
}

/// `/claims`: the caller's claims for a company, oldest first, optionally by status.
pub fn claims<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    status: Option<&str>,
) -> Result<Vec<Value>, StoreError> {
    let rows = Records::new(graph, INSURANCE_CLAIM).list(scope, 500)?;
    let mut out: Vec<Value> = rows
        .into_iter()
        .filter(|record| record.get("company_id").and_then(Value::as_str) == Some(company_id))
        .filter(|record| match status {
            Some(wanted) => record.get("status").and_then(Value::as_str) == Some(wanted),
            None => true,
        })
        .map(claim_of)
        .collect();
    out.sort_by_key(|record| {
        record
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    Ok(out)
}

/// `/claims/{claim_id}/process`: settle a claim and record the decision.
pub fn process_claim<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
    claim_id: &str,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, INSURANCE_CLAIM);
    let claim = records.get(scope, claim_id)?;
    if claim.get("company_id").and_then(Value::as_str) != Some(company_id) {
        return Err(StoreError::NotFound);
    }

    let claim_amount = stored_amount(&claim, "claim_amount")?;
    let deductible = stored_amount(&claim, "deductible")?;
    let coverage_limit = stored_amount(&claim, "coverage_limit")?;

    let mut covered = claim_amount.clone();
    if !coverage_limit.is_zero() && coverage_limit < covered {
        covered = coverage_limit.clone();
    }
    covered = covered.minus(&deductible).max_zero();
    let ratio = if claim_amount.is_zero() {
        Money::zero()
    } else {
        covered.over(&claim_amount).round(4)
    };

    let status = if covered.is_zero() {
        "denied"
    } else {
        "approved"
    };
    records.update(scope, claim_id, &json!({"status": status}))?;

    Ok(json!({
        "claim_id": claim_id,
        "company_id": company_id,
        "status": status,
        "covered_amount": wire(&covered.round(2)),
        "deductible_applied": wire(&deductible),
        "settlement_amount": wire(&covered.round(2)),
        "coverage_ratio": ratio.to_f64_lossy(),
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
    fn filing_keeps_the_money_exact_and_the_status_filed() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "claim-1"}})]);
        file_claim(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "policy_number": "POL-9",
                "claim_type": "property",
                "incident_date": "2026-01-04",
                "claim_amount": "12345678901234.56",
                "deductible": 500,
                "coverage_limit": 0,
                "supporting_docs": ["photo.jpg"],
            }),
        )
        .unwrap();
        let (cypher, params) = graph.last();
        assert!(cypher.contains("CREATE (x:InsuranceClaim)"), "{cypher}");
        assert_eq!(
            params["props"]["claim_amount"],
            json!("12345678901234.560000")
        );
        assert_eq!(params["props"]["status"], json!("filed"));
        assert_eq!(
            params["props"]["supporting_docs_json"],
            json!("[\"photo.jpg\"]")
        );
    }

    #[test]
    fn a_claim_is_capped_by_coverage_then_reduced_by_the_deductible() {
        let graph = FakeGraph::returning(vec![json!({"x": {
            "id": "claim-1",
            "company_id": "c-1",
            "claim_amount": "10000",
            "deductible": "300",
            "coverage_limit": "8000",
        }})]);
        let out = process_claim(&graph, &scope(), "c-1", "claim-1").unwrap();
        // 8000 cap, less the 300 deductible: 7700, and 0.77 of the claim
        assert_eq!(out["covered_amount"], json!(7700.0));
        assert_eq!(out["status"], json!("approved"));
        assert_eq!(out["coverage_ratio"], json!(0.77));
    }

    #[test]
    fn a_deductible_larger_than_the_claim_denies_it_rather_than_going_negative() {
        let graph = FakeGraph::returning(vec![json!({"x": {
            "id": "claim-2",
            "company_id": "c-1",
            "claim_amount": "100",
            "deductible": "250",
            "coverage_limit": "0",
        }})]);
        let out = process_claim(&graph, &scope(), "c-1", "claim-2").unwrap();
        assert_eq!(out["covered_amount"], json!(0.0));
        assert_eq!(out["status"], json!("denied"));
        assert_eq!(out["coverage_ratio"], json!(0.0));
    }

    #[test]
    fn another_books_claim_is_not_found() {
        let graph = FakeGraph::returning(vec![json!({"x": {
            "id": "claim-3",
            "company_id": "c-2",
            "claim_amount": "100",
            "deductible": "0",
            "coverage_limit": "0",
        }})]);
        assert_eq!(
            process_claim(&graph, &scope(), "c-1", "claim-3").unwrap_err(),
            StoreError::NotFound
        );
    }
}
