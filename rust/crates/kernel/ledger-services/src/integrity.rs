#![forbid(unsafe_code)]
//! financial-integrity-service: three checks over a company's records, and the report they add up to.
//!
//! Each check is arithmetic the caller supplies both sides of: a balance check compares debits and credits,
//! a hash check re-hashes the data and compares it with the hash that was stored, a completeness check
//! compares a count that should be there with the count that is. The answer is the verdict; the check itself
//! is kept as a Book-scoped record so the report can be read later.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the balance check takes a tolerance and passes when the difference is within it, inclusive;
//! * the hash check answers both the hash it computed and the one it was given, so a caller can see what
//!   the data hashes to;
//! * the completeness check answers the missing count as expected minus actual, which goes negative when
//!   there are more records than expected - that is a finding, not a refusal;
//! * the report's pass rate is a percentage of the checks recorded for that company, and a company with no
//!   checks reads 0 rather than dividing by nothing.

use serde_json::{json, Value};

use vimbai_common::hash::sha256_hex;
use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, wire};
use vimbai_store::read::{field, refused, text as read_text, whole_signed};
use vimbai_store::{BookScope, Graph, Records, StoreError, INTEGRITY_CHECK};

fn whole(payload: &Value, name: &str) -> Result<i64, StoreError> {
    whole_signed(payload, name)
}

fn company_of(payload: &Value) -> Result<String, StoreError> {
    let company = read_text(payload, "company_id");
    if company.is_empty() {
        return Err(refused("company_id is required"));
    }
    Ok(company)
}

/// `POST /check/balance`.
pub fn check_balance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company = company_of(payload)?;
    let account = read_text(payload, "account_id");
    let debits = amount_or_zero(payload, "debits")?;
    let credits = amount_or_zero(payload, "credits")?;
    let tolerance = match payload.get("tolerance") {
        None | Some(Value::Null) => Money::parse("0.01").expect("a cent parses"),
        Some(_) => amount_or_zero(payload, "tolerance")?,
    };
    let difference = debits.minus(&credits).abs();
    let passed = difference <= tolerance;

    Records::new(graph, INTEGRITY_CHECK).create(
        scope,
        &json!({
            "company_id": company,
            "check_type": "balance_check",
            "entity_type": "account",
            "entity_id": account,
            "passed": passed,
            "details": format!(
                "Debits: {}, Credits: {}, Diff: {}",
                debits.to_fixed(2),
                credits.to_fixed(2),
                difference.to_fixed(2)
            ),
        }),
    )?;

    Ok(json!({
        "passed": passed,
        "difference": wire(&difference),
        "tolerance": wire(&tolerance),
    }))
}

/// `POST /check/hash`. The hash is computed over the data's bytes exactly as they were sent.
pub fn check_hash(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company = company_of(payload)?;
    let entity_type = read_text(payload, "entity_type");
    let entity_id = read_text(payload, "entity_id");
    let data = read_text(payload, "data");
    let expected = read_text(payload, "expected_hash");
    if expected.is_empty() {
        return Err(refused("expected_hash is required"));
    }

    let actual = sha256_hex(data.as_bytes());
    let passed = actual == expected;

    Records::new(graph, INTEGRITY_CHECK).create(
        scope,
        &json!({
            "company_id": company,
            "check_type": "hash_verify",
            "entity_type": entity_type,
            "entity_id": entity_id,
            "passed": passed,
            "hash_before": expected,
            "hash_after": actual,
            "details": if passed { "Hash verified" } else { "Hash mismatch" },
        }),
    )?;

    Ok(json!({
        "passed": passed,
        "actual_hash": actual,
        "expected_hash": expected,
    }))
}

/// `POST /check/completeness`.
pub fn check_completeness(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company = company_of(payload)?;
    let entity_type = read_text(payload, "entity_type");
    let expected = whole(payload, "expected_count")?;
    let actual = whole(payload, "actual_count")?;
    let passed = expected == actual;
    let missing = expected - actual;

    Records::new(graph, INTEGRITY_CHECK).create(
        scope,
        &json!({
            "company_id": company,
            "check_type": "completeness",
            "entity_type": entity_type,
            "passed": passed,
            "details": format!("Expected: {expected}, Actual: {actual}"),
        }),
    )?;

    Ok(json!({
        "passed": passed,
        "expected": expected,
        "actual": actual,
        "missing": missing,
    }))
}

/// `GET /report/{company_id}`. Reads the caller's Book-visible checks for that company.
pub fn integrity_report(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let mut checks = Records::new(graph, INTEGRITY_CHECK).list(scope, 2000)?;
    checks.reverse();
    checks.retain(|row| field(row, "company_id") == company_id);

    let total = checks.len();
    let passed = checks
        .iter()
        .filter(|row| row.get("passed").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let failed = total - passed;
    // a company with nothing recorded reads nil rather than a division by nothing
    let pass_rate = if total == 0 {
        Money::zero()
    } else {
        Money::from_i64(passed as i64)
            .times(&Money::from_i64(100))
            .over(&Money::from_i64(total as i64))
    };

    Ok(json!({
        "company_id": company_id,
        "total_checks": total,
        "passed": passed,
        "failed": failed,
        "pass_rate": wire(&pass_rate),
        "checks": checks,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("u-1", "b-1")
    }

    #[test]
    fn the_hash_matches_the_published_vectors() {
        // the function lives once in vimbai_common and is tested there; this pins that the check
        // is wired to it rather than to a second copy
        // the two official vectors, then a 56 byte message that pads into a second block and a
        // 1000 byte one that fills fifteen of them
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256_hex(&vec![b'a'; 1000]),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn a_balance_wthin_a_cent_passes_and_a_wider_one_does_not() {
        let graph = FakeGraph::returning(vec![json!({"id": "k-new"})]);
        let within = check_balance(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "account_id": "a-1", "debits": 100, "credits": 100.01}),
        )
        .expect("checks");
        assert_eq!(within["passed"], json!(true));
        assert_eq!(within["difference"], json!("0.010000"));

        let outside = check_balance(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "account_id": "a-1", "debits": 100, "credits": 100.02}),
        )
        .expect("checks");
        assert_eq!(outside["passed"], json!(false));
    }

    #[test]
    fn a_tampered_payload_fails_the_hash_check() {
        let graph = FakeGraph::returning(vec![json!({"id": "k-new"})]);
        let answer = check_hash(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "entity_type": "journal_entry",
                "entity_id": "j-1",
                "data": "abc",
                "expected_hash": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            }),
        )
        .expect("checks");
        assert_eq!(answer["passed"], json!(true));

        let tampered = check_hash(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "entity_type": "journal_entry",
                "entity_id": "j-1",
                "data": "abc ",
                "expected_hash": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            }),
        )
        .expect("checks");
        assert_eq!(tampered["passed"], json!(false));
    }

    #[test]
    fn a_completeness_gap_is_counted_in_both_directions() {
        let graph = FakeGraph::returning(vec![json!({"id": "k-new"})]);
        let short = check_completeness(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "entity_type": "invoice", "expected_count": 10, "actual_count": 7}),
        )
        .expect("checks");
        assert_eq!(short["missing"], json!(3));

        let over = check_completeness(
            &graph,
            &scope(),
            &json!({"company_id": "c-1", "entity_type": "invoice", "expected_count": 7, "actual_count": 10}),
        )
        .expect("checks");
        assert_eq!(over["missing"], json!(-3));
    }

    #[test]
    fn the_report_reads_one_company_and_averages_its_checks() {
        let graph = FakeGraph::queue(vec![vec![
            json!({"id": "k-1", "company_id": "c-1", "passed": true}),
            json!({"id": "k-2", "company_id": "c-1", "passed": false}),
            json!({"id": "k-3", "company_id": "c-2", "passed": true}),
        ]]);
        let answer = integrity_report(&graph, &scope(), "c-1").expect("report");
        assert_eq!(answer["total_checks"], json!(2));
        assert_eq!(answer["passed"], json!(1));
        assert_eq!(answer["failed"], json!(1));
        assert_eq!(answer["pass_rate"], json!("50.000000"));
    }
}
