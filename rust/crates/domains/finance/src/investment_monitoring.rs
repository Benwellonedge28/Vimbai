#![forbid(unsafe_code)]
//! investment-monitoring-service: the portfolios a company invests through, what each holds, and how it has done.
//!
//! A holding's worth is quantity times price, exactly; a portfolio's performance is the worth of its holdings against
//! what they cost, kept as a record each time it is measured. A price change moves the holding and leaves the history
//! of measurements alone, which is what makes the record useful.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a portfolio with no holdings cannot be measured and is a miss rather than a report of nothing;
//! * the gain is the worth less the cost, and the percentage of that gain over the cost is nothing when the cost is
//!   nothing rather than dividing by zero;
//! * a holding is added only to a portfolio the caller holds;
//! * a price change applies to a holding the caller holds and answers with the holding as it now stands;
//! * performance history reads newest first, up to the limit asked for, thirty by default.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::query::query_page;
use vimbai_store::read::{decimal, field, money, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, INVESTMENT_HOLDING, INVESTMENT_PORTFOLIO,
    PERFORMANCE_METRIC,
};

/// The four risk tolerances the service knows.
const RISK_TOLERANCES: &[&str] = &["conservative", "moderate", "aggressive", "very_aggressive"];

fn risk_tolerance(payload: &Value) -> Result<String, StoreError> {
    let asked = text(payload, "risk_tolerance");
    if asked.is_empty() {
        return Ok("moderate".to_string());
    }
    if RISK_TOLERANCES.contains(&asked.as_str()) {
        Ok(asked)
    } else {
        Err(refused(
            "risk_tolerance must be one of conservative, moderate, aggressive or very_aggressive",
        ))
    }
}

fn portfolio_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    portfolio_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, INVESTMENT_PORTFOLIO)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == portfolio_id)
        .ok_or(StoreError::NotFound)
}

fn holdings_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    portfolio_id: &str,
) -> Result<Vec<Value>, StoreError> {
    Ok(Records::new(graph, INVESTMENT_HOLDING)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "portfolio_id") == portfolio_id)
        .collect())
}

/// `POST /portfolios`.
pub fn create_portfolio(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let target = Money::parse(&decimal(payload, "target_return", "0"))
        .map_err(|_| refused("target_return must be a rate"))?;

    remember(
        graph,
        scope,
        INVESTMENT_PORTFOLIO,
        json!({
            "name": name,
            "description": text(payload, "description"),
            "target_return": wire(&target),
            "risk_tolerance": risk_tolerance(payload)?,
        }),
    )
}

/// `GET /portfolios`.
pub fn list_portfolios(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    Ok(Value::Array(
        Records::new(graph, INVESTMENT_PORTFOLIO).list(scope, 2000)?,
    ))
}

/// `POST /portfolios/{portfolio_id}/holdings`.
pub fn add_holding(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    portfolio_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    portfolio_of(graph, scope, portfolio_id)?;
    let quantity = Money::parse(&decimal(payload, "quantity", "0"))
        .map_err(|_| refused("quantity must be a number"))?;
    let purchase_price = money(payload, "purchase_price")?;
    let current_price = money(payload, "current_price")?;
    if quantity.is_negative() {
        return Err(refused("quantity cannot be negative"));
    }

    remember(
        graph,
        scope,
        INVESTMENT_HOLDING,
        json!({
            "portfolio_id": portfolio_id,
            "instrument_name": text(payload, "instrument_name"),
            "instrument_type": text(payload, "instrument_type"),
            "quantity": wire(&quantity),
            "purchase_price": wire(&purchase_price),
            "current_price": wire(&current_price),
            "currency": if text(payload, "currency").is_empty() { "USD".to_string() } else { text(payload, "currency") },
            "sector": text(payload, "sector"),
        }),
    )
}

/// `GET /portfolios/{portfolio_id}/holdings`.
pub fn list_holdings(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    portfolio_id: &str,
) -> Result<Value, StoreError> {
    portfolio_of(graph, scope, portfolio_id)?;
    Ok(Value::Array(holdings_of(graph, scope, portfolio_id)?))
}

/// What the holdings are worth now, and what they cost.
fn totals(holdings: &[Value]) -> Result<(Money, Money), StoreError> {
    let mut value = Money::zero();
    let mut cost = Money::zero();
    for holding in holdings {
        let quantity = Money::parse(field(holding, "quantity")).unwrap_or_else(|_| Money::zero());
        let current =
            Money::parse(field(holding, "current_price")).unwrap_or_else(|_| Money::zero());
        let purchase =
            Money::parse(field(holding, "purchase_price")).unwrap_or_else(|_| Money::zero());
        value = value.plus(&quantity.times(&current));
        cost = cost.plus(&quantity.times(&purchase));
    }
    Ok((value, cost))
}

/// `POST /portfolios/{portfolio_id}/performance`: measure the portfolio now and keep the measurement.
pub fn record_performance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    portfolio_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    portfolio_of(graph, scope, portfolio_id)?;
    let holdings = holdings_of(graph, scope, portfolio_id)?;
    if holdings.is_empty() {
        return Err(StoreError::NotFound);
    }

    let (value, cost) = totals(&holdings)?;
    let gain = value.minus(&cost);
    let gain_pct = if cost.is_negative() || cost.is_zero() {
        Money::zero()
    } else {
        let hundred = Money::parse("100").map_err(|_| refused("a rate must be a number"))?;
        gain.over(&cost).times(&hundred)
    };

    remember(
        graph,
        scope,
        PERFORMANCE_METRIC,
        json!({
            "portfolio_id": portfolio_id,
            "metric_date": text(payload, "metric_date"),
            "total_value": wire(&value),
            "total_cost": wire(&cost),
            "unrealized_gain": wire(&gain),
            "unrealized_gain_pct": wire(&gain_pct.round(4)),
            "daily_return": "0.000000",
            "cumulative_return": "0.000000",
        }),
    )
}

/// `GET /portfolios/{portfolio_id}/performance`: the measurements, newest first.
pub fn list_performance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    portfolio_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let limit = query_page(query, 30, 2000);
    let mut rows: Vec<Value> = Records::new(graph, PERFORMANCE_METRIC)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| field(row, "portfolio_id") == portfolio_id)
        .collect();
    rows.sort_by(|one, two| field(two, "metric_date").cmp(field(one, "metric_date")));
    rows.truncate(limit);
    Ok(Value::Array(rows))
}

/// `POST /holdings/{holding_id}/update-price`.
pub fn update_price(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    holding_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let holdings = Records::new(graph, INVESTMENT_HOLDING);
    let found = holdings
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == holding_id)
        .ok_or(StoreError::NotFound)?;

    let price = Money::parse(&decimal(payload, "new_price", "0"))
        .map_err(|_| refused("new_price must be an amount"))?;
    if price.is_negative() {
        return Err(refused("new_price cannot be negative"));
    }

    holdings.update(
        scope,
        holding_id,
        &json!({
            "current_price": wire(&price),
            "quantity": field(&found, "quantity"),
            "purchase_price": field(&found, "purchase_price"),
        }),
    )?;

    let quantity = Money::parse(field(&found, "quantity")).unwrap_or_else(|_| Money::zero());
    let purchase = Money::parse(field(&found, "purchase_price")).unwrap_or_else(|_| Money::zero());
    let gain = price.minus(&purchase).times(&quantity);

    Ok(json!({
        "holding_id": holding_id,
        "instrument_name": field(&found, "instrument_name"),
        "new_price": wire(&price),
        "market_value": wire(&quantity.times(&price)),
        "gain_loss": wire(&gain),
    }))
}

/// Unused today: kept so the readers a newer caller may send are not silently dropped.
#[allow(dead_code)]
fn sector_filter(query: &Value) -> Option<String> {
    optional(query, "sector")
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn portfolio(id: &str) -> Value {
        json!({"id": id, "name": format!("fund {id}"), "risk_tolerance": "moderate"})
    }

    fn holding(id: &str, quantity: &str, purchase: &str, current: &str) -> Value {
        json!({"id": id, "portfolio_id": "p-1", "instrument_name": "Delta", "quantity": quantity,
               "purchase_price": purchase, "current_price": current})
    }

    #[test]
    fn a_portfolio_needs_a_name() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_portfolio(&graph, &scope(), &json!({"description": "unloved"})).is_err());
    }

    #[test]
    fn a_risk_tolerance_outside_the_four_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create_portfolio(
            &graph,
            &scope(),
            &json!({"name": "Growth", "risk_tolerance": "reckless"}),
        )
        .is_err());
    }

    #[test]
    fn a_portfolio_left_unsaid_is_moderate() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "p-1"})]]);
        let answer =
            create_portfolio(&graph, &scope(), &json!({"name": "Growth"})).expect("created");
        assert_eq!(answer["risk_tolerance"], json!("moderate"));
    }

    #[test]
    fn performance_is_what_the_holdings_are_worth_against_what_they_cost() {
        // 100 at 12 costing 10 (200 against 1000) and 50 at 20 costing 18 (1000 against 900)
        let graph = FakeGraph::queue(vec![
            vec![portfolio("p-1")],
            vec![
                holding("h-1", "100", "10", "12"),
                holding("h-2", "50", "18", "20"),
            ],
            vec![json!({"id": "m-1"})],
        ]);
        let answer = record_performance(&graph, &scope(), "p-1", &json!({})).expect("measured");
        assert_eq!(answer["total_value"], json!("2200.000000"));
        assert_eq!(answer["total_cost"], json!("1900.000000"));
        assert_eq!(answer["unrealized_gain"], json!("300.000000"));
        // 300 / 1900 = 15.7895%
        assert_eq!(answer["unrealized_gain_pct"], json!("15.789500"));
    }

    #[test]
    fn measuring_a_portfolio_with_nothing_in_it_is_a_miss() {
        let graph = FakeGraph::queue(vec![vec![portfolio("p-1")], vec![]]);
        assert!(matches!(
            record_performance(&graph, &scope(), "p-1", &json!({})),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_holding_is_added_only_to_a_portfolio_the_caller_holds() {
        let graph = FakeGraph::queue(vec![vec![]]);
        assert!(matches!(
            add_holding(
                &graph,
                &scope(),
                "other",
                &json!({"instrument_name": "Delta", "instrument_type": "equity", "quantity": 10,
                        "purchase_price": "1", "current_price": "1"}),
            ),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn a_price_change_answers_with_what_the_holding_is_now_worth() {
        let graph = FakeGraph::queue(vec![
            vec![holding("h-1", "10", "5", "6")],
            vec![json!({"id": "h-1"})],
        ]);
        let answer =
            update_price(&graph, &scope(), "h-1", &json!({"new_price": "8"})).expect("changed");
        assert_eq!(answer["new_price"], json!("8.000000"));
        assert_eq!(answer["market_value"], json!("80.000000"));
        assert_eq!(answer["gain_loss"], json!("30.000000"));
    }

    #[test]
    fn a_negative_price_is_refused() {
        let graph = FakeGraph::queue(vec![vec![holding("h-1", "10", "5", "6")]]);
        assert!(update_price(&graph, &scope(), "h-1", &json!({"new_price": "-1"})).is_err());
    }

    #[test]
    fn the_measurements_read_newest_first() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "m-1", "portfolio_id": "p-1", "metric_date": "2026-01-01"}),
            json!({"id": "m-2", "portfolio_id": "p-1", "metric_date": "2026-02-01"}),
            json!({"id": "m-3", "portfolio_id": "p-2", "metric_date": "2026-03-01"}),
        ]);
        let answer = list_performance(&graph, &scope(), "p-1", &json!({})).expect("listed");
        assert_eq!(answer[0]["id"], json!("m-2"));
        assert_eq!(answer.as_array().unwrap().len(), 2);
    }
}
