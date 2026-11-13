#![forbid(unsafe_code)]
//! budget-service: the master budget, what it is doing against actual, and the projections drawn from it.
//!
//! Variance is always actual less budget, so a negative variance is money not spent. For a cost line that is
//! favourable, which is why the analysis counts a negative variance as the favourable side and a positive one as
//! unfavourable. A line is watched when it is more than five percent away from its budget, and flagged as significant
//! at ten.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an account that carries no actual is measured at 95% of its budget, which is the shipped stand-in rather than a
//!   guess at nil;
//! * a variance against a nil budget is nil rather than a division;
//! * the budget as a whole is on track while it is inside five percent either way, and needs attention outside that;
//! * the detailed analysis reports a net variance of the unfavourable side less the favourable one, so a budget that
//!   underran reports negative;
//! * a forecast starts from the sum of the history, grows by the default rate unless told otherwise, is multiplied by
//!   the quarter's seasonal factor, and carries a confidence band of a tenth either way;
//! * the rolling forecast's base figures are the shipped placeholders, not derived from anything, so the endpoint is a
//!   shape rather than a model - and its optimistic and pessimistic arms bend revenue and expenses in opposite
//!   directions, which is the only part that can be read as a scenario.

use crate::figures::{amount, hundred, rounded};
use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::read::{decimal, field, optional, refused, text};
use vimbai_store::StoreError;

fn five() -> Money {
    Money::parse("5").unwrap_or_else(|_| Money::zero())
}

fn ten() -> Money {
    Money::parse("10").unwrap_or_else(|_| Money::zero())
}

/// The percentage one amount is of another, and nothing when the other is nothing.
fn percent(part: &Money, whole: &Money) -> Money {
    if whole.is_zero() || whole.is_negative() {
        Money::zero()
    } else {
        part.over(whole).times(&hundred())
    }
}

fn on_track(when_percent: &Money) -> &'static str {
    let size = if when_percent.is_negative() {
        when_percent.abs()
    } else {
        when_percent.clone()
    };
    if size.as_decimal() < five().as_decimal() {
        "on_track"
    } else {
        "attention_needed"
    }
}

/// `POST /prepare`: the summary line above the detail.
pub fn prepare(payload: &Value) -> Result<Value, StoreError> {
    let revenue = amount(payload, "revenue", "0")?;
    let cogs = amount(payload, "cogs", "0")?;
    let opex = amount(payload, "opex", "0")?;
    let gross_profit = revenue.minus(&cogs);
    let ebitda = gross_profit.minus(&opex);

    Ok(json!({
        "company_id": text(payload, "company_id"),
        "revenue": rounded(&revenue),
        "cogs": rounded(&cogs),
        "gross_profit": rounded(&gross_profit),
        "opex": rounded(&opex),
        "ebitda": rounded(&ebitda),
    }))
}

/// `POST /budget`: the account-level budget, with each account's variance.
pub fn create_budget(payload: &Value) -> Result<Value, StoreError> {
    let accounts = payload
        .get("accounts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let ninety_five = Money::parse("0.95").unwrap_or_else(|_| Money::zero());

    let mut items = Vec::new();
    let mut total_budget = Money::zero();
    let mut total_actual = Money::zero();

    for account in &accounts {
        let budgeted = Money::parse(&decimal(account, "budgeted_amount", "0"))
            .map_err(|_| refused("budgeted_amount must be an amount"))?;
        // an account carrying no actual is measured at 95% of its budget, as the service shipped
        let actual = match optional(account, "actual_amount") {
            Some(text) => {
                Money::parse(&text).map_err(|_| refused("actual_amount must be an amount"))?
            }
            None => budgeted.times(&ninety_five),
        };
        let variance = actual.minus(&budgeted);
        let variance_pct = percent(&variance, &budgeted);

        items.push(json!({
            "account_id": text(account, "account_id"),
            "account_name": text(account, "account_name"),
            "category": text(account, "category"),
            "budgeted_amount": rounded(&budgeted),
            "actual_amount": rounded(&actual),
            "variance": rounded(&variance),
            "variance_percentage": rounded(&variance_pct),
        }));
        total_budget = total_budget.plus(&budgeted);
        total_actual = total_actual.plus(&actual);
    }

    let total_variance = total_actual.minus(&total_budget);
    let variance_pct = percent(&total_variance, &total_budget);

    Ok(json!({
        "company_id": text(payload, "company_id"),
        "department_id": text(payload, "department_id"),
        "fiscal_year": payload.get("fiscal_year").cloned().unwrap_or(Value::Null),
        "period": text(payload, "period"),
        "total_budget": rounded(&total_budget),
        "total_actual": rounded(&total_actual),
        "total_variance": rounded(&total_variance),
        "variance_percentage": rounded(&variance_pct),
        "items": items,
        "status": on_track(&variance_pct),
    }))
}

/// `POST /monitor`: one line, measured.
pub fn monitor(payload: &Value) -> Result<Value, StoreError> {
    let budget = amount(payload, "budget", "0")?;
    let actual = amount(payload, "actual", "0")?;
    let variance = actual.minus(&budget);
    let pct = percent(&variance, &budget);

    Ok(json!({
        "company_id": text(payload, "company_id"),
        "budget": rounded(&budget),
        "actual": rounded(&actual),
        "variance": rounded(&variance),
        "variance_pct": rounded(&pct),
        "status": on_track(&pct),
    }))
}

/// `POST /analyze`: every line measured, the ones past ten percent lifted out, and what to do about them.
pub fn analyze(payload: &Value) -> Result<Value, StoreError> {
    let items = payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut analysis = Vec::new();
    let mut significant = Vec::new();
    let mut total_budget = Money::zero();
    let mut total_actual = Money::zero();

    for item in &items {
        let budget = Money::parse(&decimal(item, "budget", "0"))
            .map_err(|_| refused("budget must be an amount"))?;
        let actual = Money::parse(&decimal(item, "actual", "0"))
            .map_err(|_| refused("actual must be an amount"))?;
        let variance = actual.minus(&budget);
        let variance_pct = percent(&variance, &budget);

        analysis.push(json!({
            "item_id": text(item, "item_id"),
            "item_name": text(item, "item_name"),
            "category": text(item, "category"),
            "budget": rounded(&budget),
            "actual": rounded(&actual),
            "variance": rounded(&variance),
            "variance_pct": rounded(&variance_pct),
            "favorable": variance.is_negative(),
        }));
        total_budget = total_budget.plus(&budget);
        total_actual = total_actual.plus(&actual);

        let size = if variance_pct.is_negative() {
            variance_pct.abs()
        } else {
            variance_pct.clone()
        };
        if size.as_decimal() > ten().as_decimal() {
            significant.push(json!({
                "item": text(item, "item_name"),
                "variance": rounded(&variance),
                "variance_pct": rounded(&variance_pct),
            }));
        }
    }

    let total_variance = total_actual.minus(&total_budget);
    let total_variance_pct = percent(&total_variance, &total_budget);

    let mut recommendations = Vec::new();
    let total_size = if total_variance_pct.is_negative() {
        total_variance_pct.abs()
    } else {
        total_variance_pct.clone()
    };
    if total_size.as_decimal() > five().as_decimal() {
        recommendations.push("Overall budget variance exceeds 5% — investigate drivers");
    }
    if significant.len() > 5 {
        recommendations.push("Multiple significant variances — review exception items");
    }

    Ok(json!({
        "company_id": text(payload, "company_id"),
        "period": text(payload, "period"),
        "variance_summary": {
            "total_budget": rounded(&total_budget),
            "total_actual": rounded(&total_actual),
            "total_variance": rounded(&total_variance),
            "variance_pct": rounded(&total_variance_pct),
            "cost_center": text(payload, "cost_center"),
        },
        "item_analysis": analysis,
        "significant_variances": significant,
        "recommendations": recommendations,
    }))
}

/// `POST /variance-analysis`: the two sides of the variance, what fell outside tolerance, and the standing actions.
pub fn detailed_variance(payload: &Value) -> Result<Value, StoreError> {
    let results = payload
        .get("actual_results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tolerance = Money::parse(&decimal(payload, "tolerance_percentage", "5"))
        .map_err(|_| refused("tolerance_percentage must be a rate"))?;

    let mut favorable = Money::zero();
    let mut unfavorable = Money::zero();
    let mut outside = Vec::new();

    for item in &results {
        let budget = Money::parse(&decimal(item, "budget", "0"))
            .map_err(|_| refused("budget must be an amount"))?;
        let actual = Money::parse(&decimal(item, "actual", "0"))
            .map_err(|_| refused("actual must be an amount"))?;
        let variance = actual.minus(&budget);
        let variance_pct = percent(&variance, &budget);

        let size = if variance_pct.is_negative() {
            variance_pct.abs()
        } else {
            variance_pct.clone()
        };
        if size.as_decimal() > tolerance.as_decimal() {
            outside.push(json!({
                "account_id": item.get("account_id").cloned().unwrap_or(Value::Null),
                "description": text(item, "description"),
                "budget": rounded(&budget),
                "actual": rounded(&actual),
                "variance": rounded(&variance),
                "variance_pct": rounded(&variance_pct),
            }));
        }

        if variance.is_negative() {
            favorable = favorable.plus(&variance.abs());
        } else {
            unfavorable = unfavorable.plus(&variance);
        }
    }

    // the net is the unfavourable side less the favourable one, so an underspend reads as a negative net
    let net = unfavorable.minus(&favorable);

    Ok(json!({
        "company_id": text(payload, "company_id"),
        "budget_id": text(payload, "budget_id"),
        "total_variance": rounded(&net),
        "favorable_variance": rounded(&favorable),
        "unfavorable_variance": rounded(&unfavorable),
        "items_outside_tolerance": outside,
        "root_causes": {
            "revenue": "Market conditions changed",
            "costs": "Supply chain disruptions",
        },
        "corrective_actions": [
            "Review pricing strategy",
            "Renegotiate supplier contracts",
            "Implement cost controls",
        ],
    }))
}

fn power(base: f64, exponent: i32) -> f64 {
    base.powi(exponent)
}

fn scaled(value: &Money, factor: f64) -> Money {
    Money::from_f64_shortest(value.to_f64_lossy() * factor)
        .map(|money| money.round(2))
        .unwrap_or_else(|_| value.clone())
}

/// `POST /forecast`: growth on the sum of the history, bent by the quarter's seasonal factor.
pub fn forecast(payload: &Value) -> Result<Value, StoreError> {
    let periods = payload
        .get("periods")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    let history = payload
        .get("historical_data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut base = Money::zero();
    for period in &history {
        base = base.plus(
            &Money::parse(&decimal(period, "value", "0"))
                .map_err(|_| refused("value must be an amount"))?,
        );
    }

    let default_growth = Money::parse("0.03").unwrap_or_else(|_| Money::zero());
    let growth = match payload
        .get("growth_rates")
        .and_then(|rates| rates.get("default"))
    {
        Some(value) => Money::parse(value.to_string().trim_matches('"'))
            .map_err(|_| refused("growth_rates must be rates"))?,
        None => default_growth,
    };
    let growth_factor = 1.0 + growth.to_f64_lossy();

    let mut projections = Vec::new();
    let mut intervals = Vec::new();
    for period in 1..=periods {
        // the quarter a period falls in, three periods to a quarter, keyed the way the service keyed it
        let quarter = format!("Q{}", (period - 1) / 3 + 1);
        let seasonal = match payload
            .get("seasonal_factors")
            .and_then(|factors| factors.get(&quarter))
        {
            Some(value) => Money::parse(value.to_string().trim_matches('"'))
                .map_err(|_| refused("seasonal_factors must be rates"))?,
            None => Money::parse("1").unwrap_or_else(|_| Money::zero()),
        };

        let projected = scaled(
            &base,
            power(growth_factor, period as i32) * seasonal.to_f64_lossy(),
        );
        projections.push(json!({
            "period": period,
            "projected_value": rounded(&projected),
            "growth_rate": json!(growth.to_fixed(6)),
            "seasonal_factor": json!(seasonal.to_fixed(6)),
        }));
        intervals.push(json!({
            "period": period,
            "lower": rounded(&scaled(&projected, 0.9)),
            "upper": rounded(&scaled(&projected, 1.1)),
        }));
    }

    Ok(json!({
        "company_id": text(payload, "company_id"),
        "forecast_method": if text(payload, "forecast_method").is_empty() {
            "growth_rate".to_string()
        } else {
            text(payload, "forecast_method")
        },
        "periods_forecast": periods,
        "projections": projections,
        "confidence_intervals": intervals,
        "accuracy_score": 0.92,
    }))
}

/// `POST /rolling-forecast`: the shipped placeholder base, bent three ways.
pub fn rolling_forecast(payload: &Value) -> Result<Value, StoreError> {
    let forward = payload
        .get("forward_periods")
        .and_then(Value::as_i64)
        .unwrap_or(12)
        .max(0);
    let today = text(payload, "today");
    if today.is_empty() {
        return Err(refused("today must be an ISO date"));
    }

    let million = Money::parse("1000000").unwrap_or_else(|_| Money::zero());
    let expenses_base = Money::parse("700000").unwrap_or_else(|_| Money::zero());
    let profit_base = Money::parse("300000").unwrap_or_else(|_| Money::zero());

    let mut base_arm = Vec::new();
    for period in 1..=forward {
        base_arm.push(json!({
            "period": period,
            "revenue": rounded(&scaled(&million, power(1.05, period as i32))),
            "expenses": rounded(&scaled(&expenses_base, power(1.03, period as i32))),
            "profit": rounded(&scaled(&profit_base, power(1.07, period as i32))),
        }));
    }

    let mut optimistic = Vec::new();
    let mut pessimistic = Vec::new();
    for row in &base_arm {
        let revenue = Money::parse(field(row, "revenue")).unwrap_or_else(|_| Money::zero());
        let expenses = Money::parse(field(row, "expenses")).unwrap_or_else(|_| Money::zero());

        let better_revenue = scaled(&revenue, 1.1);
        let better_expenses = scaled(&expenses, 0.95);
        optimistic.push(json!({
            "period": field(row, "period"),
            "revenue": rounded(&better_revenue),
            "expenses": rounded(&better_expenses),
            "profit": rounded(&better_revenue.minus(&better_expenses)),
        }));

        let worse_revenue = scaled(&revenue, 0.9);
        let worse_expenses = scaled(&expenses, 1.05);
        pessimistic.push(json!({
            "period": field(row, "period"),
            "revenue": rounded(&worse_revenue),
            "expenses": rounded(&worse_expenses),
            "profit": rounded(&worse_revenue.minus(&worse_expenses)),
        }));
    }

    Ok(json!({
        "company_id": text(payload, "company_id"),
        "forecast_id": format!("RF-{}", today.replace('-', "")),
        "base_actual": 1_000_000.0,
        "forward_projections": base_arm,
        "scenarios": {
            "base": base_arm,
            "optimistic": optimistic,
            "pessimistic": pessimistic,
        },
        "recommended_scenario": "base",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prepared_summary_walks_from_revenue_to_ebitda() {
        let answer = prepare(&json!({"company_id": "c-1", "revenue": "1000000",
                                     "cogs": "600000", "opex": "250000"}))
        .expect("prepared");
        assert_eq!(answer["gross_profit"], json!("400000.00"));
        assert_eq!(answer["ebitda"], json!("150000.00"));
    }

    #[test]
    fn an_account_without_an_actual_is_measured_at_ninety_five_percent_of_budget() {
        let answer = create_budget(&json!({
            "company_id": "c-1", "department_id": "d-1", "fiscal_year": 2026, "period": "Q1",
            "accounts": [{"account_id": "a-1", "account_name": "Rent", "category": "opex",
                          "budgeted_amount": "2000"}]
        }))
        .expect("budgeted");
        assert_eq!(answer["items"][0]["actual_amount"], json!("1900.00"));
        assert_eq!(answer["items"][0]["variance"], json!("-100.00"));
        assert_eq!(answer["items"][0]["variance_percentage"], json!("-5.00"));
        assert_eq!(answer["status"], json!("attention_needed"));
    }

    #[test]
    fn a_budget_inside_five_percent_is_on_track() {
        let answer = create_budget(&json!({
            "company_id": "c-1",
            "accounts": [{"account_name": "Rent", "budgeted_amount": "2000", "actual_amount": "1980"}]
        }))
        .expect("budgeted");
        assert_eq!(answer["status"], json!("on_track"));
        assert_eq!(answer["total_variance"], json!("-20.00"));
    }

    #[test]
    fn a_variance_against_a_nil_budget_is_nil_rather_than_a_division() {
        let answer = monitor(&json!({"company_id": "c-1", "budget": "0", "actual": "500"}))
            .expect("monitored");
        assert_eq!(answer["variance_pct"], json!("0.00"));
        assert_eq!(answer["variance"], json!("500.00"));
    }

    #[test]
    fn a_line_more_than_ten_percent_out_is_lifted_out_as_significant() {
        let answer = analyze(&json!({
            "company_id": "c-1", "period": "Q1",
            "items": [
                {"item_id": "i-1", "item_name": "Travel", "category": "opex", "budget": "1000", "actual": "1300"},
                {"item_id": "i-2", "item_name": "Power", "category": "opex", "budget": "1000", "actual": "1050"}
            ]
        }))
        .expect("analysed");
        assert_eq!(answer["significant_variances"].as_array().unwrap().len(), 1);
        assert_eq!(answer["significant_variances"][0]["item"], json!("Travel"));
        assert_eq!(answer["item_analysis"][1]["favorable"], json!(false));
        // 350 over 2000 is 17.5%, past five, so the standing advice is offered
        assert_eq!(answer["recommendations"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn an_underspend_is_favourable_and_the_net_reports_it_as_negative() {
        let answer = detailed_variance(&json!({
            "company_id": "c-1", "budget_id": "b-1", "tolerance_percentage": 5,
            "actual_results": [
                {"account_id": "a-1", "budget": "1000", "actual": "900"},
                {"account_id": "a-2", "budget": "1000", "actual": "1100"}
            ]
        }))
        .expect("analysed");
        assert_eq!(answer["favorable_variance"], json!("100.00"));
        assert_eq!(answer["unfavorable_variance"], json!("100.00"));
        assert_eq!(answer["total_variance"], json!("0.00"));
        assert_eq!(
            answer["items_outside_tolerance"].as_array().unwrap().len(),
            2
        );
        assert_eq!(answer["corrective_actions"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_forecast_grows_the_sum_of_the_history_by_the_default_rate() {
        let answer = forecast(&json!({
            "company_id": "c-1", "periods": 2,
            "historical_data": [{"value": "100"}, {"value": "100"}]
        }))
        .expect("forecast");
        // 200 at 3% is 206, then 212.18
        assert_eq!(answer["projections"][0]["projected_value"], json!("206.00"));
        assert_eq!(answer["projections"][1]["projected_value"], json!("212.18"));
        assert_eq!(answer["confidence_intervals"][1]["lower"], json!("190.96"));
        assert_eq!(answer["projections"][0]["growth_rate"], json!("0.030000"));
    }

    #[test]
    fn a_seasonal_factor_lands_on_the_quarter_its_period_falls_in() {
        let answer = forecast(&json!({
            "company_id": "c-1", "periods": 4,
            "historical_data": [{"value": "1000"}],
            "growth_rates": {"default": 0},
            "seasonal_factors": {"Q1": 1.2, "Q2": 0.8}
        }))
        .expect("forecast");
        // period 4 is the first of Q2
        assert_eq!(
            answer["projections"][0]["projected_value"],
            json!("1200.00")
        );
        assert_eq!(answer["projections"][3]["projected_value"], json!("800.00"));
    }

    #[test]
    fn the_rolling_forecast_bends_revenue_and_expenses_in_opposite_directions() {
        let answer = rolling_forecast(&json!({"company_id": "c-1", "forward_periods": 1,
                                              "today": "2026-11-13"}))
        .expect("forecast");
        assert_eq!(answer["forecast_id"], json!("RF-20261113"));
        assert_eq!(
            answer["scenarios"]["base"][0]["revenue"],
            json!("1050000.00")
        );
        assert_eq!(
            answer["scenarios"]["optimistic"][0]["revenue"],
            json!("1155000.00")
        );
        assert_eq!(
            answer["scenarios"]["pessimistic"][0]["revenue"],
            json!("945000.00")
        );
        assert_eq!(answer["recommended_scenario"], json!("base"));
    }

    #[test]
    fn a_rolling_forecast_without_a_date_is_refused_rather_than_dated_by_the_clock() {
        assert!(rolling_forecast(&json!({"company_id": "c-1"})).is_err());
    }
}
