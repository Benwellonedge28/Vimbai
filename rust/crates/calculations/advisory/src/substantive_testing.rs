//! Substantive testing, ported from substantive-testing-service.
//!
//! Misstatements are decimals: a population of large balances accumulates exactly, so
//! the total is the sum the auditor can tie out rather than a float's approximation.
//!
//! Two behaviours are the service's and are kept deliberately:
//!
//! * the projection factor is the *population count over sample size*, not the value
//!   ratio an auditor might expect, so a 10-item population sampled at 4 projects the
//!   misstatement by 2.5x whatever the amounts are;
//! * the conclusion turns "qualified" once the projected misstatement reaches a tenth
//!   of the threshold, which is far below the threshold itself.

use serde_json::{json, Value};
use vimbai_common::money::amount_of as money_of;
use vimbai_common::Money;

const TESTS_PERFORMED: [&str; 8] = [
    "existence",
    "completeness",
    "valuation",
    "rights_and_obligations",
    "accuracy",
    "cutoff",
    "classification",
    "presentation",
];

/// Test a sample of a population and project what it suggests about the whole.
pub fn evaluate(payload: &Value) -> Result<Value, String> {
    let population = payload
        .get("population")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let sample_size = payload
        .get("sample_size")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let threshold = money_of(payload.get("misstatement_threshold"))?;

    let mut total = Money::zero();
    let mut exceptions = Vec::new();

    for item in population.iter().take(sample_size) {
        let misstatement = money_of(item.get("misstatement"))?.abs();
        total = total.plus(&misstatement);
        if misstatement > threshold {
            exceptions.push(json!({
                "item_id": item.get("item_id").and_then(Value::as_str).unwrap_or(""),
                "amount": money_of(item.get("amount"))?.to_fixed(2),
                "misstatement": misstatement.to_fixed(2),
            }));
        }
    }

    // a missing sample size projects at the population's own size, as the service did
    let factor = if sample_size == 0 {
        Money::from_i64(1)
    } else {
        Money::from_i64(population.len() as i64).over(&Money::from_i64(sample_size as i64))
    };
    let projected = total.times(&factor).round(2);

    let trigger = threshold
        .times(&Money::from_i64(1))
        .over(&Money::from_i64(10));
    let conclusion = if projected < trigger {
        "unqualified"
    } else {
        "qualified"
    };

    Ok(json!({
        "audit_id": payload.get("audit_id").and_then(Value::as_str).unwrap_or(""),
        "account_id": payload.get("account_id").and_then(Value::as_str).unwrap_or(""),
        "tests_performed": TESTS_PERFORMED,
        "items_tested": sample_size,
        "exceptions_found": exceptions.len(),
        "total_misstatement": total.round(2).to_fixed(2),
        "projected_misstatement": projected.to_fixed(2),
        "conclusion": conclusion,
        "recommendation": if exceptions.len() > 3 {
            "Extend testing"
        } else {
            "Conclude on account balance"
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(population: Value, sample_size: u64, threshold: &str) -> Value {
        json!({
            "audit_id": "a1",
            "account_id": "acc1",
            "assertions": ["existence"],
            "population": population,
            "sample_size": sample_size,
            "misstatement_threshold": threshold,
        })
    }

    #[test]
    fn the_projection_uses_the_count_ratio() {
        let items = json!([
            {"item_id": "1", "amount": "1000", "misstatement": "100"},
            {"item_id": "2", "amount": "1000", "misstatement": "0"},
            {"item_id": "3", "amount": "1000", "misstatement": "0"},
            {"item_id": "4", "amount": "1000", "misstatement": "0"},
            {"item_id": "5", "amount": "1000", "misstatement": "0"},
        ]);
        let out = evaluate(&payload(items, 4, "50")).unwrap();
        assert_eq!(out["total_misstatement"], "100.00");
        // five items sampled at four: 1.25x, not the 5,000 over 4,000 an auditor expects
        assert_eq!(out["projected_misstatement"], "125.00");
        assert_eq!(out["exceptions_found"], 1);
    }

    #[test]
    fn a_projection_past_a_tenth_of_the_threshold_qualifies_the_account() {
        let out = evaluate(&payload(
            json!([{"item_id": "1", "misstatement": "20"}]),
            1,
            "100",
        ))
        .unwrap();
        assert_eq!(out["projected_misstatement"], "20.00");
        assert_eq!(out["conclusion"], "qualified");
    }

    #[test]
    fn large_balances_accumulate_exactly() {
        let out = evaluate(&payload(
            json!([
                {"item_id": "1", "misstatement": "123456789012345678901.01"},
                {"item_id": "2", "misstatement": "-0.01"},
            ]),
            2,
            "1",
        ))
        .unwrap();
        // both signs count toward the total, and the last cent survives
        assert_eq!(out["total_misstatement"], "123456789012345678901.02");
    }

    #[test]
    fn a_missing_sample_size_tests_nothing_but_still_projects() {
        let out = evaluate(&payload(json!([{"misstatement": "5"}]), 0, "10")).unwrap();
        assert_eq!(out["items_tested"], 0);
        assert_eq!(out["total_misstatement"], "0.00");
        assert_eq!(out["conclusion"], "unqualified");
    }
}
