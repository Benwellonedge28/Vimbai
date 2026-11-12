#![forbid(unsafe_code)]
//! issued-share-capital-service, on the store.
//!
//! The register of what has been issued: each issue records how many shares went out and what they
//! raised, the shareholder register says who holds them, and the summary adds the two figures up.
//!
//! Kept as shipped, and it is a real quirk rather than an accident of sampling: the issue entry
//! credits share capital at **one currency unit per share**, whatever class or price the shares were,
//! and puts everything else to premium. So a preference issue and an ordinary issue at the same price
//! post the same capital, and an issue below one unit posts negative premium. The service hard-coded
//! that nominal figure rather than taking one, and a records system that silently "fixed" it would
//! disagree with every entry it has already written. It is pinned by a test instead.
//!
//! The summary is shares issued and proceeds raised, both summed across issues.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::{BookScope, Graph, Records, StoreError, SHAREHOLDER, SHARE_ISSUE};

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn count(payload: &Value, key: &str) -> i64 {
    payload
        .get(key)
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(raw) => raw.trim().parse::<i64>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

/// The nominal the issue entry credits share capital at: one unit a share, as shipped.
pub const NOMINAL_PER_SHARE: i64 = 1;

/// What an issue raises.
pub fn issue_proceeds(price: &Money, shares: i64) -> Money {
    price.times(&Money::from_i64(shares))
}

/// The capital an issue credits, at the hard-coded nominal.
pub fn issue_capital(shares: i64) -> Money {
    Money::from_i64(shares * NOMINAL_PER_SHARE)
}

/// `/issue`.
pub fn record_issue<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let shares = count(payload, "shares_issued");
    let price = amount_or_zero(payload, "issue_price")?;
    let share_class = text(payload, "share_class");
    let date = text(payload, "issue_date");
    let proceeds = issue_proceeds(&price, shares);
    let capital = issue_capital(shares);
    let premium = proceeds.minus(&capital);
    let zero = Money::zero();

    let journal = json!({
        "date": date,
        "description": format!("Issue of {shares} {share_class} shares at {}", wire(&price)),
        "entries": [
            {
                "account_code": "1000",
                "description": "Bank",
                "debit": wire(&proceeds),
                "credit": wire(&zero),
            },
            {
                "account_code": "3200",
                "description": "Share Capital",
                "debit": wire(&zero),
                "credit": wire(&capital),
            },
            {
                "account_code": "3210",
                "description": "Share Premium",
                "debit": wire(&zero),
                "credit": wire(&premium),
            },
        ],
        "reference": "ISS",
    });

    Records::new(graph, SHARE_ISSUE).create(
        scope,
        &json!({
            "company_id": text(payload, "company_id"),
            "issue_date": date,
            "share_class": share_class,
            "shares_issued": shares,
            "issue_price": wire(&price),
            "total_proceeds": wire(&proceeds),
            "journal_entry": journal,
        }),
    )
}

fn company_rows<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let rows = Records::new(graph, kind).list(scope, 1000)?;
    Ok(rows
        .into_iter()
        .filter(|row| row.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect())
}

/// `/shareholders/{company_id}`.
pub fn list_shareholders<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    Ok(json!({
        "shareholders": company_rows(graph, scope, SHAREHOLDER, company_id)?,
    }))
}

/// `/issues/{company_id}`.
pub fn list_issues<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    Ok(json!({
        "issues": company_rows(graph, scope, SHARE_ISSUE, company_id)?,
    }))
}

/// `/summary/{company_id}`: shares issued and proceeds raised.
pub fn capital_summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let issues = company_rows(graph, scope, SHARE_ISSUE, company_id)?;
    let mut shares = 0i64;
    let mut proceeds = Money::zero();
    for row in &issues {
        shares += row
            .get("shares_issued")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        proceeds = proceeds.plus(&stored_amount(row, "total_proceeds")?);
    }
    Ok(json!({
        "total_shares_issued": shares,
        "total_proceeds": wire(&proceeds),
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
    fn the_issue_splits_proceeds_at_one_unit_a_share_as_shipped() {
        let graph = FakeGraph::returning(vec![json!({"id": "iss-1"})]);
        record_issue(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "shares_issued": 1000, "issue_price": "2.25",
                    "share_class": "ordinary", "issue_date": "2026-03-01"}),
        )
        .expect("issued");
        let (_, params) = graph.last();
        let written = &params["props"];
        assert_eq!(written["total_proceeds"], json!("2250.000000"));
        let lines = written["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[1]["credit"], json!("1000.000000"));
        assert_eq!(lines[2]["credit"], json!("1250.000000"));
    }

    #[test]
    fn a_preference_issue_credits_the_same_capital_because_the_nominal_is_hard_coded() {
        let graph = FakeGraph::returning(vec![json!({"id": "iss-2"})]);
        record_issue(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "shares_issued": 100, "issue_price": "5",
                    "share_class": "preference", "issue_date": "2026-03-01"}),
        )
        .expect("issued");
        let (_, params) = graph.last();
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[1]["credit"], json!("100.000000"));
        assert_eq!(lines[2]["credit"], json!("400.000000"));
    }

    #[test]
    fn an_issue_below_one_unit_posts_negative_premium() {
        let graph = FakeGraph::returning(vec![json!({"id": "iss-3"})]);
        record_issue(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "shares_issued": 10, "issue_price": "0.25",
                    "share_class": "ordinary", "issue_date": "2026-03-01"}),
        )
        .expect("issued");
        let (_, params) = graph.last();
        let lines = params["props"]["journal_entry"]["entries"]
            .as_array()
            .expect("lines");
        assert_eq!(lines[2]["credit"], json!("-7.500000"));
    }

    #[test]
    fn a_huge_issue_keeps_every_digit() {
        let graph = FakeGraph::returning(vec![json!({"id": "iss-4"})]);
        record_issue(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "shares_issued": 2,
                    "issue_price": "123456789012345678901.55", "share_class": "ordinary",
                    "issue_date": "2026-03-01"}),
        )
        .expect("issued");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["total_proceeds"],
            json!("246913578024691357803.100000")
        );
    }

    #[test]
    fn the_summary_adds_shares_and_proceeds_across_issues() {
        let graph = FakeGraph::queue(vec![vec![
            json!({"company_id": "co-1", "shares_issued": 1000, "total_proceeds": "2250"}),
            json!({"company_id": "co-1", "shares_issued": 500, "total_proceeds": "750.25"}),
            json!({"company_id": "co-2", "shares_issued": 99, "total_proceeds": "9900"}),
        ]]);
        let answer = capital_summary(&graph, &scope(), "co-1").expect("summary");
        assert_eq!(answer["total_shares_issued"], json!(1500));
        assert_eq!(answer["total_proceeds"], json!("3000.250000"));
    }
}
