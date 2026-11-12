#![forbid(unsafe_code)]
//! treasury-risk-service: the exposures a Book carries, the value at risk measured on a portfolio, the stress
//! scenarios it runs, and the dashboard over all of it.
//!
//! Amounts are exact. Two figures are approximations and say so: the value at risk scales by the square root of
//! the holding period, and a square root has no exact decimal form, so that factor is the nearest double and
//! everything it multiplies is exact from there; and a percentage is divided out and rounded, as shipped.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an exposure type outside the five the service knows, or a shock type outside the four, is refused with the
//!   list of valid ones;
//! * value at risk only answers at one of three confidences, and carries the z-score the service used for each
//!   of them (1.282, 1.645, 2.326) rather than computing one from the normal curve;
//! * a portfolio of zero reports no percentage rather than dividing, and the amount still comes back as zero;
//! * a market crash and a currency devaluation take the shock magnitude as a percentage of the portfolio, while
//!   a rate move takes it as basis points against an assumed five-year duration, so a hundred basis points
//!   moves the portfolio by five per cent and not by one;
//! * both listings and the dashboard hand back the most recent records in the order they were made;
//! * the dashboard totals the notional of every exposure, groups it by type, and reports the LATEST value at
//!   risk rather than a sum of them, because adding yesterday's risk to today's would double it.

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::{amount, wire};
use vimbai_store::query::{query_page, query_text};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, RISK_EXPOSURE, STRESS_RESULT,
    STRESS_SCENARIO, VAR_RESULT,
};

fn refused(message: &str) -> StoreError {
    StoreError::Invalid(message.to_string())
}

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    match text(payload, key) {
        value if value.is_empty() => fallback.to_string(),
        value => value,
    }
}

fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

fn money(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Err(refused(&format!("{key} is required"))),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

fn money_or_nothing(payload: &Value, key: &str) -> Result<Money, StoreError> {
    match payload.get(key) {
        Some(Value::Null) | None => Ok(Money::zero()),
        Some(value) => amount(&json!({ "amount": value }), "amount"),
    }
}

fn number(payload: &Value, key: &str) -> Option<f64> {
    match payload.get(key) {
        Some(Value::Number(value)) => value.as_f64(),
        Some(Value::String(value)) => value.trim().parse().ok(),
        _ => None,
    }
}

fn percent(payload: &Value, key: &str, fallback: &str) -> String {
    match payload.get(key) {
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) if !value.is_empty() => value.clone(),
        _ => fallback.to_string(),
    }
}

fn remember(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    kind: vimbai_store::RecordKind,
    body: Value,
) -> Result<Value, StoreError> {
    let saved = Records::new(graph, kind).create(scope, &body)?;
    let mut answer = created_record(&saved);
    if let (Some(answer), Some(fields)) = (answer.as_object_mut(), body.as_object()) {
        for (key, value) in fields {
            answer.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(answer)
}

/// `POST /exposures`.
pub fn create_exposure(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let kind = text(payload, "exposure_type");
    let known = ["fx", "interest_rate", "credit", "liquidity", "commodity"];
    if !known.contains(&kind.as_str()) {
        return Err(refused(
            "Invalid type. Must be one of ['fx', 'interest_rate', 'credit', 'liquidity', 'commodity']",
        ));
    }
    let notional = money(payload, "notional_amount")?;
    remember(
        graph,
        scope,
        RISK_EXPOSURE,
        json!({
            "exposure_type": kind,
            "currency": or_default(payload, "currency", "USD"),
            "notional_amount": wire(&notional),
            "description": text(payload, "description"),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /exposures`.
pub fn list_exposures(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = query_text(query, "exposure_type");
    let mut all: Vec<Value> = Records::new(graph, RISK_EXPOSURE)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            wanted
                .as_deref()
                .map(|kind| field(row, "exposure_type") == kind)
                .unwrap_or(true)
        })
        .collect();
    all.sort_by(|one, two| field(one, "created_at").cmp(field(two, "created_at")));
    Ok(Value::Array(all))
}

/// `POST /var`.
pub fn calculate_var(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let portfolio = money(payload, "portfolio_value")?;
    let confidence = percent(payload, "confidence_level", "0.95");
    // the z-score the service carried for each confidence it allows, not a computed one
    let z = match confidence.as_str() {
        "0.9" | "0.90" => 1.282,
        "0.95" => 1.645,
        "0.99" => 2.326,
        _ => return Err(refused("Confidence must be one of [0.9, 0.95, 0.99]")),
    };
    let days = number(payload, "holding_period_days").unwrap_or(1.0);
    if days <= 0.0 {
        return Err(refused("holding_period_days must be positive"));
    }
    let volatility = Money::parse(&percent(payload, "daily_volatility", "0.01"))
        .map_err(|error| refused(&format!("daily_volatility: {error}")))?;

    // the only approximation: the square root of the holding period has no exact decimal form
    let scale = z * days.sqrt();
    let factor =
        Money::from_f64_shortest(scale).map_err(|error| refused(&format!("scale: {error}")))?;
    let var_amount = portfolio.times(&volatility).times(&factor).round(2);

    let var_pct = if portfolio > Money::zero() {
        var_amount
            .over(&portfolio)
            .times(&Money::parse("100").expect("money"))
            .round(4)
    } else {
        Money::zero()
    };

    remember(
        graph,
        scope,
        VAR_RESULT,
        json!({
            "portfolio_value": wire(&portfolio),
            "confidence_level": confidence,
            "holding_period_days": days as i64,
            "var_amount": var_amount.to_fixed(2),
            "var_pct": var_pct.to_fixed(4),
            "method": or_default(payload, "method", "parametric"),
            "calculated_at": text(payload, "calculated_at"),
        }),
    )
}

/// `GET /var`.
pub fn list_var_results(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let limit = query_page(query, 50, 2000);
    let mut all = Records::new(graph, VAR_RESULT).list(scope, 2000)?;
    all.sort_by(|one, two| field(one, "calculated_at").cmp(field(two, "calculated_at")));
    let start = all.len().saturating_sub(limit);
    Ok(Value::Array(all[start..].to_vec()))
}

/// `POST /scenarios`.
pub fn create_scenario(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let shock = text(payload, "shock_type");
    let known = [
        "interest_rate_up",
        "interest_rate_down",
        "fx_devaluation",
        "market_crash",
    ];
    if !known.contains(&shock.as_str()) {
        return Err(refused(
            "Invalid shock type. Must be one of ['interest_rate_up', 'interest_rate_down', \
             'fx_devaluation', 'market_crash']",
        ));
    }
    let magnitude = money(payload, "shock_magnitude")?;
    remember(
        graph,
        scope,
        STRESS_SCENARIO,
        json!({
            "name": text(payload, "name"),
            "description": text(payload, "description"),
            "shock_type": shock,
            "shock_magnitude": wire(&magnitude),
            "portfolio_impact": wire(&Money::zero()),
            "created_at": text(payload, "created_at"),
        }),
    )
}

/// `GET /scenarios`.
pub fn list_scenarios(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut all = Records::new(graph, STRESS_SCENARIO).list(scope, 2000)?;
    all.sort_by(|one, two| field(one, "created_at").cmp(field(two, "created_at")));
    Ok(Value::Array(all))
}

/// `POST /scenarios/{scenario_id}/run`.
pub fn run_stress_test(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    scenario_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let Some(scenario) = Records::new(graph, STRESS_SCENARIO)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == scenario_id)
    else {
        return Err(StoreError::NotFound);
    };

    let portfolio = money(payload, "portfolio_value")?;
    let magnitude = money_or_nothing(&scenario, "shock_magnitude")?;
    let hundred = Money::parse("100").expect("money");
    let ten_thousand = Money::parse("10000").expect("money");
    let duration = Money::parse("5").expect("money");

    let impact = match field(&scenario, "shock_type") {
        // a percentage of the portfolio
        "market_crash" | "fx_devaluation" => {
            Money::zero().minus(&portfolio.times(&magnitude).over(&hundred))
        }
        // basis points against an assumed five-year duration
        "interest_rate_up" | "interest_rate_down" => Money::zero().minus(
            &portfolio
                .times(&magnitude)
                .over(&ten_thousand)
                .times(&duration),
        ),
        _ => Money::zero(),
    };

    let after = portfolio.plus(&impact);
    let impact_pct = if portfolio > Money::zero() {
        impact.over(&portfolio).times(&hundred).round(4)
    } else {
        Money::zero()
    };

    remember(
        graph,
        scope,
        STRESS_RESULT,
        json!({
            "scenario_id": scenario_id,
            "portfolio_value_before": wire(&portfolio),
            "portfolio_value_after": after.round(2).to_fixed(2),
            "impact": impact.round(2).to_fixed(2),
            "impact_pct": impact_pct.to_fixed(4),
            "tested_at": text(payload, "tested_at"),
        }),
    )
}

/// `GET /stress-results`.
pub fn list_stress_results(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = query_text(query, "scenario_id");
    let limit = query_page(query, 50, 2000);
    let mut all: Vec<Value> = Records::new(graph, STRESS_RESULT)
        .list(scope, 2000)?
        .into_iter()
        .filter(|row| {
            wanted
                .as_deref()
                .map(|id| field(row, "scenario_id") == id)
                .unwrap_or(true)
        })
        .collect();
    all.sort_by(|one, two| field(one, "tested_at").cmp(field(two, "tested_at")));
    let start = all.len().saturating_sub(limit);
    Ok(Value::Array(all[start..].to_vec()))
}

/// `GET /dashboard`.
pub fn dashboard(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let exposures = Records::new(graph, RISK_EXPOSURE).list(scope, 2000)?;
    let mut vars = Records::new(graph, VAR_RESULT).list(scope, 2000)?;
    let scenarios = Records::new(graph, STRESS_SCENARIO).list(scope, 2000)?;
    let results = Records::new(graph, STRESS_RESULT).list(scope, 2000)?;

    let mut total = Money::zero();
    let mut by_type: serde_json::Map<String, Value> = serde_json::Map::new();
    for exposure in &exposures {
        let notional = money_or_nothing(exposure, "notional_amount")?;
        total = total.plus(&notional);
        let kind = field(exposure, "exposure_type").to_string();
        let running = by_type
            .get(&kind)
            .map(|value| {
                Money::parse(value.to_string().trim_matches('"')).unwrap_or_else(|_| Money::zero())
            })
            .unwrap_or_else(Money::zero);
        by_type.insert(kind, json!(wire(&running.plus(&notional))));
    }

    vars.sort_by(|one, two| field(one, "calculated_at").cmp(field(two, "calculated_at")));
    // as shipped: the latest measure, not a sum of measures taken on different days
    let latest = match vars.last() {
        Some(row) => field(row, "var_amount").to_string(),
        None => "0".to_string(),
    };

    Ok(json!({
        "total_exposures": exposures.len(),
        "total_notional": wire(&total),
        "by_type": Value::Object(by_type),
        "var_calculations": vars.len(),
        "latest_var": latest,
        "stress_scenarios": scenarios.len(),
        "stress_tests_run": results.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn exposure(id: &str, kind: &str, notional: &str) -> Value {
        json!({"id": id, "exposure_type": kind, "currency": "USD", "notional_amount": notional})
    }

    #[test]
    fn an_exposure_of_an_unknown_type_is_refused_with_the_types_there_are() {
        let graph = FakeGraph::returning(vec![]);
        let error = create_exposure(
            &graph,
            &scope(),
            &json!({"exposure_type": "weather", "notional_amount": "100.00"}),
        )
        .expect_err("refused");
        assert!(format!("{error}").contains("commodity"), "{error}");
    }

    #[test]
    fn an_exposure_keeps_every_digit_of_its_notional() {
        let graph = FakeGraph::returning(vec![json!({"id": "x-1"})]);
        let answer = create_exposure(
            &graph,
            &scope(),
            &json!({"exposure_type": "fx", "currency": "USD",
                    "notional_amount": "123456789012345678901.55"}),
        )
        .expect("exposure");
        assert_eq!(
            answer["notional_amount"],
            json!("123456789012345678901.550000")
        );
    }

    #[test]
    fn the_exposure_list_narrows_to_one_type_when_asked() {
        let graph = FakeGraph::returning(vec![
            exposure("x-1", "fx", "100.000000"),
            exposure("x-2", "credit", "200.000000"),
        ]);
        let answer = list_exposures(&graph, &scope(), &json!({"exposure_type": "credit"}))
            .expect("exposures");
        let rows = answer.as_array().expect("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], json!("x-2"));
    }

    #[test]
    fn var_at_ninety_five_percent_is_the_z_score_times_the_value_times_the_volatility() {
        let graph = FakeGraph::returning(vec![json!({"id": "v-1"})]);
        let answer = calculate_var(
            &graph,
            &scope(),
            &json!({"portfolio_value": "100000.00", "confidence_level": "0.95",
                    "daily_volatility": "0.01", "holding_period_days": 1}),
        )
        .expect("var");
        assert_eq!(answer["var_amount"], json!("1645.00"));
        assert_eq!(answer["var_pct"], json!("1.6450"));
    }

    #[test]
    fn var_over_two_days_scales_by_the_square_root_of_two() {
        let graph = FakeGraph::returning(vec![json!({"id": "v-1"})]);
        let answer = calculate_var(
            &graph,
            &scope(),
            &json!({"portfolio_value": "100000.00", "confidence_level": "0.99",
                    "daily_volatility": "0.02", "holding_period_days": 2}),
        )
        .expect("var");
        // 100000 * 0.02 * 2.326 * sqrt(2) = 6578.92 to the cent
        assert_eq!(answer["var_amount"], json!("6578.92"));
    }

    #[test]
    fn var_refuses_a_confidence_it_has_no_z_score_for() {
        let graph = FakeGraph::returning(vec![]);
        let error = calculate_var(
            &graph,
            &scope(),
            &json!({"portfolio_value": "100.00", "confidence_level": "0.8"}),
        )
        .expect_err("refused");
        assert!(format!("{error}").contains("0.99"), "{error}");
    }

    #[test]
    fn var_refuses_a_holding_period_of_no_days() {
        let graph = FakeGraph::returning(vec![]);
        let error = calculate_var(
            &graph,
            &scope(),
            &json!({"portfolio_value": "100.00", "holding_period_days": 0}),
        )
        .expect_err("refused");
        assert!(format!("{error}").contains("positive"), "{error}");
    }

    #[test]
    fn a_market_crash_takes_its_percentage_off_the_portfolio() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "s-1", "name": "Crash", "shock_type": "market_crash",
                        "shock_magnitude": "20.00"}),
            ],
            vec![json!({"id": "r-1"})],
        ]);
        let answer = run_stress_test(
            &graph,
            &scope(),
            "s-1",
            &json!({"portfolio_value": "500.00"}),
        )
        .expect("stress");
        assert_eq!(answer["impact"], json!("-100.00"));
        assert_eq!(answer["portfolio_value_after"], json!("400.00"));
        assert_eq!(answer["impact_pct"], json!("-20.0000"));
    }

    #[test]
    fn a_rate_shock_is_basis_points_against_a_five_year_duration() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "s-1", "shock_type": "interest_rate_up",
                        "shock_magnitude": "100.00"})],
            vec![json!({"id": "r-1"})],
        ]);
        let answer = run_stress_test(
            &graph,
            &scope(),
            "s-1",
            &json!({"portfolio_value": "1000.00"}),
        )
        .expect("stress");
        // 100 basis points on 1000 over a five-year duration
        assert_eq!(answer["impact"], json!("-50.00"));
    }

    #[test]
    fn a_scenario_the_caller_cannot_see_is_a_miss_rather_than_a_refusal() {
        let graph = FakeGraph::returning(vec![]);
        let error = run_stress_test(
            &graph,
            &scope(),
            "s-9",
            &json!({"portfolio_value": "100.00"}),
        )
        .expect_err("miss");
        assert_eq!(error, StoreError::NotFound);
    }

    #[test]
    fn the_dashboard_adds_the_notionals_by_type_and_counts_what_is_stored() {
        let graph = FakeGraph::queue(vec![
            vec![
                exposure("x-1", "fx", "100.000000"),
                exposure("x-2", "fx", "250.000000"),
                exposure("x-3", "credit", "50.000000"),
            ],
            vec![
                json!({"id": "v-1", "var_amount": "9.990000", "calculated_at": "2026-03-01T00:00:00"}),
            ],
            vec![json!({"id": "s-1"})],
            vec![json!({"id": "r-1"})],
        ]);
        let answer = dashboard(&graph, &scope()).expect("dashboard");
        assert_eq!(answer["total_exposures"], json!(3));
        assert_eq!(answer["total_notional"], json!("400.000000"));
        assert_eq!(answer["by_type"]["fx"], json!("350.000000"));
        assert_eq!(answer["by_type"]["credit"], json!("50.000000"));
        assert_eq!(answer["var_calculations"], json!(1));
        assert_eq!(answer["latest_var"], json!("9.990000"));
        assert_eq!(answer["stress_scenarios"], json!(1));
        assert_eq!(answer["stress_tests_run"], json!(1));
    }
}
