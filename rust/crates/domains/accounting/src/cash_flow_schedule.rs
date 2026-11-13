#![forbid(unsafe_code)]
//! cash-flow-service, on the store.
//!
//! A project's cash flow, year by year: what comes in, what goes out, and the net. The schedules are
//! kept, so they can be listed and read back later.
//!
//! The arithmetic, all exact:
//!
//! * one entry per inflow year, numbered from one, with that year's outflow where the caller gave one
//!   and zero where they did not,
//! * net flow is inflow less outflow, and totals are the sums of the entries,
//! * the standalone calculators pad the shorter list with zeros so both sides line up.
//!
//! Kept as shipped: the initial investment is recorded and never subtracted. A project that costs a
//! million to start reports its net flow as if it cost nothing, which is what the service did and what
//! the schedules already stored say. Correcting it here would make a stored summary disagree with the
//! figure it published.
//!
//! Also as shipped: reading a schedule that does not exist answers with an error body and a 200, not a
//! 404. The route keeps that shape because callers already branch on the body.

use serde_json::{json, Value};
use vimbai_common::Money;

use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::text;
use vimbai_store::{BookScope, Graph, Records, StoreError, CASH_FLOW_SUMMARY};

/// A list of amounts, accepting either a bare `[1, 2]` body or `{"key": [1, 2]}`.
fn amounts(payload: &Value, key: &str) -> Result<Vec<Money>, StoreError> {
    let list = match payload {
        Value::Array(rows) => rows.clone(),
        other => other
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    list.iter()
        .map(|row| match row {
            Value::Number(_) | Value::String(_) => {
                amount_or_zero(&json!({ "amount": row }), "amount")
            }
            _ => Ok(Money::zero()),
        })
        .collect()
}

fn sum(rows: &[Money]) -> Money {
    rows.iter()
        .fold(Money::zero(), |total, row| total.plus(row))
}

/// One entry per inflow year, with that year's outflow or zero.
pub fn build_entries(inflows: &[Money], outflows: &[Money]) -> Vec<Value> {
    inflows
        .iter()
        .enumerate()
        .map(|(index, inflow)| {
            let outflow = outflows.get(index).cloned().unwrap_or_else(Money::zero);
            json!({
                "year": index + 1,
                "inflow": wire(inflow),
                "outflow": wire(&outflow),
                "net_flow": wire(&inflow.minus(&outflow)),
            })
        })
        .collect()
}

/// Pad both sides to the same length so net flows can be paired.
pub fn pair(inflows: &[Money], outflows: &[Money]) -> (Vec<Money>, Vec<Money>) {
    let length = inflows.len().max(outflows.len());
    let mut left = inflows.to_vec();
    let mut right = outflows.to_vec();
    left.resize(length, Money::zero());
    right.resize(length, Money::zero());
    (left, right)
}

/// `/create`.
pub fn create<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let inflows = amounts(payload, "yearly_inflows")?;
    let outflows = amounts(payload, "yearly_outflows")?;
    let entries = build_entries(&inflows, &outflows);

    let mut inflow_total = Money::zero();
    let mut outflow_total = Money::zero();
    let mut net_total = Money::zero();
    for entry in &entries {
        inflow_total = inflow_total.plus(&amount_or_zero(entry, "inflow")?);
        outflow_total = outflow_total.plus(&amount_or_zero(entry, "outflow")?);
        net_total = net_total.plus(&amount_or_zero(entry, "net_flow")?);
    }

    Records::new(graph, CASH_FLOW_SUMMARY).create(
        scope,
        &json!({
            "project_name": text(payload, "project_name"),
            // recorded, and deliberately not taken off the totals
            "initial_investment": wire(&amount_or_zero(payload, "initial_investment")?),
            "entries": entries,
            "total_inflows": wire(&inflow_total),
            "total_outflows": wire(&outflow_total),
            "total_net_flow": wire(&net_total),
        }),
    )
}

/// `/inflows` and `/outflows`: a total of one list.
pub fn total_of(payload: &Value, key: &str) -> Result<Value, StoreError> {
    let rows = amounts(payload, key)?;
    Ok(json!({
        key: rows.iter().map(wire).collect::<Vec<_>>(),
        format!("total_{key}"): wire(&sum(&rows)),
    }))
}

/// `/net-flows`.
pub fn net_flows(payload: &Value) -> Result<Value, StoreError> {
    let inflows = amounts(payload, "inflows")?;
    let outflows = amounts(payload, "outflows")?;
    let (left, right) = pair(&inflows, &outflows);
    let nets: Vec<Money> = left.iter().zip(&right).map(|(i, o)| i.minus(o)).collect();
    Ok(json!({
        "net_flows": nets.iter().map(wire).collect::<Vec<_>>(),
        "total_net_flow": wire(&sum(&nets)),
    }))
}

/// `/cumulative-flows`.
pub fn cumulative_flows(payload: &Value) -> Result<Value, StoreError> {
    let nets = amounts(payload, "net_flows")?;
    let mut running = Money::zero();
    let mut cumulative = Vec::with_capacity(nets.len());
    for flow in &nets {
        running = running.plus(flow);
        cumulative.push(wire(&running));
    }
    Ok(json!({
        "net_flows": nets.iter().map(wire).collect::<Vec<_>>(),
        "cumulative_flows": cumulative,
    }))
}

/// `/list`.
pub fn list<G: Graph + ?Sized>(graph: &G, scope: &BookScope) -> Result<Value, StoreError> {
    let rows = Records::new(graph, CASH_FLOW_SUMMARY).list(scope, 1000)?;
    Ok(json!({ "cash_flows": rows }))
}

/// `/get/{id}`: the body carries the error, as it always has.
pub fn get<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    cash_flow_id: &str,
) -> Result<Value, StoreError> {
    match Records::new(graph, CASH_FLOW_SUMMARY).get(scope, cash_flow_id) {
        Ok(found) => Ok(found),
        Err(StoreError::NotFound) => Ok(json!({ "error": "Cash flow not found" })),
        Err(other) => Err(other),
    }
}

/// The initial investment as recorded, for callers that want to apply it themselves.
pub fn recorded_investment<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    cash_flow_id: &str,
) -> Result<Money, StoreError> {
    let found = Records::new(graph, CASH_FLOW_SUMMARY).get(scope, cash_flow_id)?;
    stored_amount(&found, "initial_investment")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    #[test]
    fn there_is_one_entry_per_inflow_year_numbered_from_one() {
        let entries = build_entries(&[money("100"), money("200")], &[money("50")]);
        assert_eq!(entries[0]["year"], json!(1));
        assert_eq!(entries[0]["net_flow"], json!("50.000000"));
        assert_eq!(entries[1]["year"], json!(2));
        assert_eq!(entries[1]["outflow"], json!("0.000000"));
        assert_eq!(entries[1]["net_flow"], json!("200.000000"));
    }

    #[test]
    fn both_sides_are_padded_so_net_flows_pair_up() {
        let (left, right) = pair(&[money("100")], &[money("10"), money("20")]);
        assert_eq!(left.len(), 2);
        assert_eq!(wire(&left[1]), "0.000000");
        assert_eq!(wire(&right[1]), "20.000000");
    }

    #[test]
    fn the_initial_investment_is_recorded_and_never_subtracted() {
        let graph = FakeGraph::returning(vec![json!({"id": "cf-1"})]);
        create(
            &graph,
            &scope(),
            &json!({"project_name": "Mill", "initial_investment": "1000000",
                    "yearly_inflows": ["100"], "yearly_outflows": ["40"]}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["initial_investment"],
            json!("1000000.000000")
        );
        // a million spent and the net flow still reports only what the years brought
        assert_eq!(params["props"]["total_net_flow"], json!("60.000000"));
    }

    #[test]
    fn a_schedule_past_double_size_keeps_every_digit() {
        let graph = FakeGraph::returning(vec![json!({"id": "cf-1"})]);
        create(
            &graph,
            &scope(),
            &json!({"project_name": "Mill",
                    "yearly_inflows": ["123456789012345678901.55"],
                    "yearly_outflows": ["0.45"]}),
        )
        .expect("created");
        let (_, params) = graph.last();
        assert_eq!(
            params["props"]["total_net_flow"],
            json!("123456789012345678901.100000")
        );
    }

    #[test]
    fn cumulative_flows_run_a_total_forward() {
        let answer = cumulative_flows(&json!({"net_flows": ["100", "-40", "10"]})).expect("flows");
        assert_eq!(
            answer["cumulative_flows"],
            json!(["100.000000", "60.000000", "70.000000"])
        );
    }

    #[test]
    fn net_flows_accept_a_bare_list_for_each_side() {
        let answer =
            net_flows(&json!({"inflows": ["100"], "outflows": ["40", "10"]})).expect("flows");
        assert_eq!(answer["net_flows"], json!(["60.000000", "-10.000000"]));
        assert_eq!(answer["total_net_flow"], json!("50.000000"));
    }

    #[test]
    fn a_missing_schedule_answers_with_the_error_body_rather_than_not_found() {
        let graph = FakeGraph::returning(Vec::new());
        let answer = get(&graph, &scope(), "nope").expect("answered");
        assert_eq!(answer["error"], json!("Cash flow not found"));
    }
}
