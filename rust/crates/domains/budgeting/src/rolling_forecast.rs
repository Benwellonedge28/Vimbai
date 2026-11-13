#![forbid(unsafe_code)]
//! rolling-forecast-service: a forecast that moves - periods appended as time passes, actuals filled in behind it, so
//! the same record always shows the next few periods and what the last few actually came to.
//!
//! The periods ride on the forecast rather than standing alone, so recording an actual rewrites the forecast. The
//! variance against a recorded actual is actual less forecast, and its percentage is against the forecast, which is nil
//! when the forecast is nil rather than a division.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a frequency outside weekly, monthly and quarterly is refused;
//! * the current period is the first period given, and empty when there are none;
//! * recording an actual for a period the forecast does not carry changes nothing and reports no variance, rather than
//!   adding the period;
//! * rolling appends the new periods and reports how many the forecast now holds;
//! * a forecast the caller does not hold is a miss, and the listing narrows by metric.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, ROLLING_FORECAST};

/// The three frequencies a forecast may roll at.
const FREQUENCIES: &[&str] = &["weekly", "monthly", "quarterly"];

fn checked_frequency(frequency: &str) -> Result<String, StoreError> {
    if FREQUENCIES.contains(&frequency) {
        Ok(frequency.to_string())
    } else {
        Err(refused(
            "Invalid frequency. Must be one of ['weekly', 'monthly', 'quarterly']",
        ))
    }
}

/// One period, with the shape the service gave it.
fn period_of(payload: &Value) -> Result<Value, StoreError> {
    let period = text(payload, "period");
    if period.is_empty() {
        return Err(refused("period is required"));
    }
    Ok(json!({
        "period": period,
        "forecast_value": wire(&Money::parse(&decimal(payload, "forecast_value", "0"))
            .map_err(|_| refused("forecast_value must be an amount"))?),
        "actual_value": Value::Null,
        "variance": Value::Null,
        "variance_pct": Value::Null,
        "is_actual": false,
    }))
}

fn periods_of(forecast: &Value) -> Vec<Value> {
    forecast
        .get("periods")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn forecast_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    forecast_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, ROLLING_FORECAST)
        .list(scope, 2000)?
        .into_iter()
        .find(|row| field(row, "id") == forecast_id)
        .ok_or(StoreError::NotFound)
}

/// `POST /forecasts`.
pub fn create(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
    payload: &Value,
) -> Result<Value, StoreError> {
    // the service took these as query parameters, so they are read from either place
    let name = {
        let asked = text(query, "name");
        if asked.is_empty() {
            text(payload, "name")
        } else {
            asked
        }
    };
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let metric = {
        let asked = text(query, "metric");
        if asked.is_empty() {
            text(payload, "metric")
        } else {
            asked
        }
    };
    let frequency = checked_frequency(&{
        let asked = text(query, "frequency");
        if asked.is_empty() {
            "monthly".to_string()
        } else {
            asked
        }
    })?;
    let horizon = field(query, "horizon_periods")
        .parse::<i64>()
        .or_else(|_| field(payload, "horizon_periods").parse::<i64>())
        .unwrap_or(12);

    let mut periods = Vec::new();
    for period in payload
        .get("forecast_values")
        .or_else(|| query.get("forecast_values"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        periods.push(period_of(&period)?);
    }
    let current = periods
        .first()
        .map(|period| text(period, "period"))
        .unwrap_or_default();

    remember(
        graph,
        scope,
        ROLLING_FORECAST,
        json!({
            "name": name,
            "metric": metric,
            "frequency": frequency,
            "horizon_periods": horizon,
            "periods": periods,
            "current_period": current,
        }),
    )
}

/// `GET /forecasts`.
pub fn list_forecasts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let wanted = optional(query, "metric").unwrap_or_default();
    Ok(Value::Array(
        Records::new(graph, ROLLING_FORECAST)
            .list(scope, 2000)?
            .into_iter()
            .filter(|row| wanted.is_empty() || field(row, "metric") == wanted)
            .collect(),
    ))
}

/// `GET /forecasts/{forecast_id}`.
pub fn forecast_by_id(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    forecast_id: &str,
) -> Result<Value, StoreError> {
    forecast_of(graph, scope, forecast_id)
}

fn percentage(part: &Money, whole: &Money) -> Money {
    if whole.is_zero() {
        Money::zero()
    } else {
        part.over(whole)
            .times(&Money::parse("100").unwrap_or_else(|_| Money::zero()))
    }
}

/// `POST /forecasts/{forecast_id}/update-actual`.
pub fn update_actual(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    forecast_id: &str,
    query: &Value,
    payload: &Value,
) -> Result<Value, StoreError> {
    let forecast = forecast_of(graph, scope, forecast_id)?;
    let period = {
        let asked = text(query, "period");
        if asked.is_empty() {
            text(payload, "period")
        } else {
            asked
        }
    };
    let actual = Money::parse(&{
        let asked = decimal(query, "actual_value", "");
        if asked.is_empty() {
            decimal(payload, "actual_value", "0")
        } else {
            asked
        }
    })
    .map_err(|_| refused("actual_value must be an amount"))?;

    let mut periods = periods_of(&forecast);
    let mut variance: Option<Money> = None;
    for entry in periods.iter_mut() {
        if field(entry, "period") != period {
            continue;
        }
        let forecast_value =
            Money::parse(field(entry, "forecast_value")).unwrap_or_else(|_| Money::zero());
        let difference = actual.minus(&forecast_value);
        if let Some(object) = entry.as_object_mut() {
            object.insert("actual_value".to_string(), json!(wire(&actual)));
            object.insert("is_actual".to_string(), json!(true));
            object.insert("variance".to_string(), json!(wire(&difference)));
            object.insert(
                "variance_pct".to_string(),
                json!(wire(&percentage(&difference, &forecast_value))),
            );
        }
        variance = Some(difference);
        break;
    }

    Records::new(graph, ROLLING_FORECAST).update(
        scope,
        forecast_id,
        &json!({"periods": periods, "current_period": field(&forecast, "current_period")}),
    )?;

    // a period the forecast does not carry changes nothing and reports no variance
    Ok(json!({
        "forecast_id": forecast_id,
        "period": period,
        "actual_value": wire(&actual),
        "variance": variance.map(|value| json!(wire(&value))).unwrap_or(Value::Null),
    }))
}

/// `POST /forecasts/{forecast_id}/roll`.
pub fn roll(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    forecast_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let forecast = forecast_of(graph, scope, forecast_id)?;
    let mut periods = periods_of(&forecast);
    let added = payload
        .get("new_periods")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for period in &added {
        periods.push(period_of(period)?);
    }

    Records::new(graph, ROLLING_FORECAST).update(
        scope,
        forecast_id,
        &json!({"periods": periods}),
    )?;

    Ok(json!({
        "forecast_id": forecast_id,
        "total_periods": periods.len(),
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
    fn a_forecast_starts_at_its_first_period() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "f-1"})]]);
        let answer = create(
            &graph,
            &scope(),
            &json!({"name": "FY26 revenue", "metric": "revenue", "frequency": "quarterly"}),
            &json!({"forecast_values": [
                {"period": "2026-01", "forecast_value": "1000"},
                {"period": "2026-02", "forecast_value": "1100"}
            ]}),
        )
        .expect("created");
        assert_eq!(answer["current_period"], json!("2026-01"));
        assert_eq!(answer["frequency"], json!("quarterly"));
        assert_eq!(answer["periods"].as_array().unwrap().len(), 2);
        assert_eq!(answer["periods"][0]["is_actual"], json!(false));
    }

    #[test]
    fn a_frequency_the_service_does_not_know_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        assert!(create(
            &graph,
            &scope(),
            &json!({"name": "FY26", "metric": "revenue", "frequency": "daily"}),
            &json!({}),
        )
        .is_err());
    }

    #[test]
    fn a_forecast_with_no_periods_falls_back_to_monthly_and_no_current_period() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "f-1"})]]);
        let answer = create(
            &graph,
            &scope(),
            &json!({"name": "FY26", "metric": "cash_flow"}),
            &json!({}),
        )
        .expect("created");
        assert_eq!(answer["frequency"], json!("monthly"));
        assert_eq!(answer["current_period"], json!(""));
        assert_eq!(answer["horizon_periods"], json!(12));
    }

    #[test]
    fn recording_an_actual_measures_it_against_the_forecast() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "f-1", "current_period": "2026-01",
                        "periods": [{"period": "2026-01", "forecast_value": "1000.000000"}]})],
            vec![json!({"id": "f-1"})],
        ]);
        let answer = update_actual(
            &graph,
            &scope(),
            "f-1",
            &json!({"period": "2026-01", "actual_value": 1100.0}),
            &json!({}),
        )
        .expect("recorded");
        // 100 over 1000 is 10%
        assert_eq!(answer["variance"], json!("100.000000"));
    }

    #[test]
    fn an_actual_for_a_period_the_forecast_does_not_carry_reports_no_variance() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "f-1", "periods": [{"period": "2026-01", "forecast_value": "1000"}]}),
            ],
            vec![json!({"id": "f-1"})],
        ]);
        let answer = update_actual(
            &graph,
            &scope(),
            "f-1",
            &json!({"period": "2026-09", "actual_value": 1000.0}),
            &json!({}),
        )
        .expect("recorded");
        assert_eq!(answer["variance"], Value::Null);
        assert_eq!(answer["period"], json!("2026-09"));
    }

    #[test]
    fn rolling_appends_and_reports_how_many_periods_the_forecast_now_holds() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "f-1", "periods": [{"period": "2026-01", "forecast_value": "1000"}]}),
            ],
            vec![json!({"id": "f-1"})],
        ]);
        let answer = roll(
            &graph,
            &scope(),
            "f-1",
            &json!({"new_periods": [{"period": "2026-02", "forecast_value": "1200"}]}),
        )
        .expect("rolled");
        assert_eq!(answer["total_periods"], json!(2));
    }

    #[test]
    fn a_forecast_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            forecast_by_id(&graph, &scope(), "f-9"),
            Err(StoreError::NotFound)
        ));
    }

    #[test]
    fn the_listing_narrows_by_metric() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "f-1", "metric": "revenue"}),
            json!({"id": "f-2", "metric": "headcount"}),
        ]);
        let answer =
            list_forecasts(&graph, &scope(), &json!({"metric": "headcount"})).expect("listed");
        assert_eq!(answer.as_array().unwrap().len(), 1);
    }
}
