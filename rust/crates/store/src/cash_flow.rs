//! cash-flow-statement-service's statements, on the store.
//!
//! A statement is a set of activity lines and the four totals derived from them: net operating,
//! net investing, net financing, then the change in cash and where cash ends up. The totals are
//! computed here rather than accepted from the caller, so a client cannot post a statement whose
//! parts do not add up, and the addition runs on `Money` so a nine-figure statement is exact.
//!
//! Statements are ledger records: they are created and read, never edited. A correction is a new
//! statement, which is why this module has no update path.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use vimbai_common::Money;

use crate::amounts::{stored_amount, wire};
use crate::{BookScope, Graph, Records, StoreError, CASH_FLOW_STATEMENT};

/// One activity line, kept as it arrived but with its amount made exact.
fn normalise_line(line: &Value) -> Result<Value, StoreError> {
    let amount = match line.get("amount") {
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|error| StoreError::Invalid(format!("amount: {error}")))?,
        Some(Value::String(text)) => {
            Money::parse(text).map_err(|error| StoreError::Invalid(format!("amount: {error}")))?
        }
        _ => {
            return Err(StoreError::Invalid(
                "a cash flow line needs an amount".into(),
            ))
        }
    };
    let inflow = line
        .get("is_inflow")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Ok(json!({
        "description": line.get("description").and_then(Value::as_str).unwrap_or(""),
        "amount": amount.to_fixed(6),
        "is_inflow": inflow,
    }))
}

/// The lines under one key, plus the net they add up to.
fn section(payload: &Value, key: &str) -> Result<(Vec<Value>, Money), StoreError> {
    let mut net = Money::zero();
    let mut lines = Vec::new();
    for line in payload
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let line = normalise_line(&line)?;
        let amount = Money::parse(line["amount"].as_str().unwrap_or("0"))
            .map_err(|error| StoreError::Invalid(format!("amount: {error}")))?;
        net = if line["is_inflow"].as_bool().unwrap_or(true) {
            net.plus(&amount)
        } else {
            net.minus(&amount)
        };
        lines.push(line);
    }
    Ok((lines, net))
}

fn lines_json(lines: &[Value]) -> String {
    serde_json::to_string(lines).unwrap_or_else(|_| "[]".to_owned())
}

/// The statement as the service answered it: exact figures, published as numbers.
///
/// The long argument list mirrors the fields the service published, one per line item; grouping
/// them into a struct would only rename the same fields.
#[allow(clippy::too_many_arguments)]
fn statement_of(
    id: &str,
    company_id: &str,
    method: &str,
    period_start: &str,
    period_end: &str,
    operating: &[Value],
    investing: &[Value],
    financing: &[Value],
    net_operating: &Money,
    net_investing: &Money,
    net_financing: &Money,
    beginning_cash: &Money,
    created_at: &str,
) -> Value {
    let net_change = net_operating.plus(net_investing).plus(net_financing);
    let ending_cash = beginning_cash.plus(&net_change);
    let published = |lines: &[Value]| -> Vec<Value> {
        lines
            .iter()
            .map(|line| {
                let amount = Money::parse(line["amount"].as_str().unwrap_or("0"))
                    .unwrap_or_else(|_| Money::zero());
                json!({
                    "description": line["description"],
                    "amount": wire(&amount),
                    "is_inflow": line["is_inflow"],
                })
            })
            .collect()
    };
    json!({
        "id": id,
        "company_id": company_id,
        "method": method,
        "period_start": period_start,
        "period_end": period_end,
        "operating_activities": published(operating),
        "investing_activities": published(investing),
        "financing_activities": published(financing),
        "net_operating": wire(net_operating),
        "net_investing": wire(net_investing),
        "net_financing": wire(net_financing),
        "net_change": wire(&net_change),
        "beginning_cash": wire(beginning_cash),
        "ending_cash": wire(&ending_cash),
        "created_at": created_at,
    })
}

/// `/generate`: compute the totals and store the statement.
pub fn generate<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = payload
        .get("company_id")
        .and_then(Value::as_str)
        .ok_or_else(|| StoreError::Invalid("company_id is required".into()))?;
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("direct");
    let period_start = payload
        .get("period_start")
        .and_then(Value::as_str)
        .unwrap_or("");
    let period_end = payload
        .get("period_end")
        .and_then(Value::as_str)
        .unwrap_or("");
    let beginning_cash = match payload.get("beginning_cash") {
        Some(Value::String(text)) => Money::parse(text)
            .map_err(|error| StoreError::Invalid(format!("beginning_cash: {error}")))?,
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|error| StoreError::Invalid(format!("beginning_cash: {error}")))?,
        _ => Money::zero(),
    };

    let (operating, net_operating) = section(payload, "operating_activities")?;
    let (investing, net_investing) = section(payload, "investing_activities")?;
    let (financing, net_financing) = section(payload, "financing_activities")?;
    let net_change = net_operating.plus(&net_investing).plus(&net_financing);

    let record = json!({
        "company_id": company_id,
        "method": method,
        "period_start": period_start,
        "period_end": period_end,
        "operating_activities": lines_json(&operating),
        "investing_activities": lines_json(&investing),
        "financing_activities": lines_json(&financing),
        "net_operating": net_operating.to_fixed(6),
        "net_investing": net_investing.to_fixed(6),
        "net_financing": net_financing.to_fixed(6),
        "net_change": net_change.to_fixed(6),
        "beginning_cash": beginning_cash.to_fixed(6),
        "ending_cash": beginning_cash.plus(&net_change).to_fixed(6),
        "created_at": payload.get("created_at").and_then(Value::as_str).unwrap_or(""),
    });
    let stored = Records::new(graph, CASH_FLOW_STATEMENT).create(scope, &record)?;
    let id = stored.get("id").and_then(Value::as_str).unwrap_or("");

    Ok(statement_of(
        id,
        company_id,
        method,
        period_start,
        period_end,
        &operating,
        &investing,
        &financing,
        &net_operating,
        &net_investing,
        &net_financing,
        &beginning_cash,
        record["created_at"].as_str().unwrap_or(""),
    ))
}

/// A stored statement, read back from the graph.
fn read_back(record: &Value) -> Result<Value, StoreError> {
    let lines = |key: &str| -> Vec<Value> {
        record
            .get(key)
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str::<Vec<Value>>(text).ok())
            .unwrap_or_default()
    };
    let text = |key: &str| record.get(key).and_then(Value::as_str).unwrap_or("");
    Ok(statement_of(
        text("id"),
        text("company_id"),
        text("method"),
        text("period_start"),
        text("period_end"),
        &lines("operating_activities"),
        &lines("investing_activities"),
        &lines("financing_activities"),
        &stored_amount(record, "net_operating")?,
        &stored_amount(record, "net_investing")?,
        &stored_amount(record, "net_financing")?,
        &stored_amount(record, "beginning_cash")?,
        text("created_at"),
    ))
}

/// The caller's statements for a company, oldest first.
fn statements<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut mine: Vec<Value> = Records::new(graph, CASH_FLOW_STATEMENT)
        .list(scope, 500)?
        .into_iter()
        .filter(|record| record.get("company_id").and_then(Value::as_str) == Some(company_id))
        .collect();
    mine.sort_by_key(|record| {
        record
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    });
    mine.iter().map(read_back).collect()
}

/// `/latest/{company_id}`: the most recent statement, or NotFound as the service answered.
pub fn latest<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    statements(graph, scope, company_id)?
        .pop()
        .ok_or(StoreError::NotFound)
}

/// `/history/{company_id}`: every statement for the company, with the count.
pub fn history<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Value, StoreError> {
    let statements = statements(graph, scope, company_id)?;
    Ok(json!({
        "company_id": company_id,
        "statements": statements,
        "total": statements.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn the_totals_come_from_the_lines_and_the_cash_ties_out() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "stmt-1"}})]);
        let out = generate(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "beginning_cash": 1000,
                "operating_activities": [
                    {"description": "Receipts", "amount": 5000, "is_inflow": true},
                    {"description": "Payments", "amount": 2000, "is_inflow": false}
                ],
                "investing_activities": [
                    {"description": "Equipment", "amount": 1500, "is_inflow": false}
                ],
                "financing_activities": [
                    {"description": "Loan", "amount": 400, "is_inflow": true}
                ],
            }),
        )
        .unwrap();

        assert_eq!(out["net_operating"], json!(3000.0));
        assert_eq!(out["net_investing"], json!(-1500.0));
        assert_eq!(out["net_financing"], json!(400.0));
        assert_eq!(out["net_change"], json!(1900.0));
        assert_eq!(out["ending_cash"], json!(2900.0));

        let (_, params) = graph.last();
        assert_eq!(params["props"]["ending_cash"], json!("2900.000000"));
    }

    #[test]
    fn a_long_statement_stays_exact() {
        let graph = FakeGraph::returning(vec![json!({"x": {"id": "stmt-2"}})]);
        let out = generate(
            &graph,
            &scope(),
            &json!({
                "company_id": "c-1",
                "beginning_cash": "0.01",
                "operating_activities": [
                    {"description": "Big", "amount": "12345678901234.56", "is_inflow": true}
                ],
            }),
        )
        .unwrap();
        assert_eq!(out["ending_cash"], json!(12345678901234.57));
    }

    #[test]
    fn no_statements_yet_is_not_found_not_an_empty_statement() {
        let graph = FakeGraph::returning(vec![]);
        assert_eq!(
            latest(&graph, &scope(), "c-1").unwrap_err(),
            StoreError::NotFound
        );
    }

    #[test]
    fn history_reports_the_count_and_the_statements() {
        let graph = FakeGraph::returning(vec![json!({"x": {
            "id": "stmt-1",
            "company_id": "c-1",
            "method": "direct",
            "operating_activities": "[{\"description\": \"Receipts\", \"amount\": \"5000.000000\", \"is_inflow\": true}]",
            "investing_activities": "[]",
            "financing_activities": "[]",
            "net_operating": "5000",
            "net_investing": "0",
            "net_financing": "0",
            "beginning_cash": "0",
            "ending_cash": "5000",
            "created_at": "2026-01-01T00:00:00+00:00"
        }})]);
        let out = history(&graph, &scope(), "c-1").unwrap();
        assert_eq!(out["total"], json!(1));
        assert_eq!(out["statements"][0]["net_operating"], json!(5000.0));
        assert_eq!(
            out["statements"][0]["operating_activities"][0]["amount"],
            json!(5000.0)
        );
    }
}
