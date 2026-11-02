//! cash-flow-forecasting-service: a daily cash forecast rolled up to weeks and
//! months, with a burn rate and a runway.
//!
//! Python quirks kept, because treasurers reconcile against these figures:
//! - only `daily` and `weekly` items are forecast; `monthly` and `annual`
//!   items are silently ignored, so a monthly rent never appears;
//! - a weekly item lands on day 1, 8, 15, ... because the check is
//!   `day % 7 == 0` against a zero-based day;
//! - weekly and monthly rollups sum the ROUNDED daily figures, so they drift
//!   from the rounded week-end balance;
//! - the runway divides start cash by the average outflow and ignores inflows
//!   entirely, and is null when the burn rate is zero.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CashFlowItem {
    #[serde(default)]
    pub category: String,
    pub amount: f64,
    #[serde(default)]
    pub frequency: String,
    #[serde(default)]
    pub certainty: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CashFlowForecastRequest {
    pub company_id: String,
    pub starting_cash: f64,
    #[serde(default)]
    pub inflows: Vec<CashFlowItem>,
    #[serde(default)]
    pub outflows: Vec<CashFlowItem>,
    #[serde(default)]
    pub forecast_period_days: i64,
    #[serde(default)]
    pub scenario: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyForecastRow {
    pub day: i64,
    pub inflow: f64,
    pub outflow: f64,
    pub net_flow: f64,
    pub ending_cash: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeriodSummary {
    /// The service publishes this as `week` for weekly buckets and `month`
    /// for monthly ones, so the seam renames it per rollup.
    pub index: i64,
    pub total_inflow: f64,
    pub total_outflow: f64,
    pub ending_cash: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioRow {
    pub ending_cash: f64,
    pub peak_need: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashFlowForecastResult {
    pub company_id: String,
    pub period_days: i64,
    pub daily_forecasts: Vec<DailyForecastRow>,
    pub weekly_summary: Vec<PeriodSummary>,
    pub monthly_summary: Vec<PeriodSummary>,
    pub ending_cash: f64,
    pub peak_cash_needed: f64,
    pub min_cash_balance: f64,
    pub cash_burn_rate: f64,
    pub runway_days: Option<f64>,
    pub scenarios: std::collections::BTreeMap<String, ScenarioRow>,
}

fn due(item: &CashFlowItem, day: i64) -> bool {
    match item.frequency.as_str() {
        "daily" => day % 1 == 0,
        "weekly" => day % 7 == 0,
        _ => false,
    }
}

fn roll_up(rows: &[DailyForecastRow], size: usize, starting_cash: f64) -> Vec<PeriodSummary> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < rows.len() {
        let end = (index + size).min(rows.len());
        let chunk = &rows[index..end];
        let inflow: f64 = chunk.iter().map(|r| r.inflow).sum();
        let outflow: f64 = chunk.iter().map(|r| r.outflow).sum();
        out.push(PeriodSummary {
            index: (index / size) as i64 + 1,
            total_inflow: py_round2(inflow),
            total_outflow: py_round2(outflow),
            ending_cash: chunk.last().map(|r| r.ending_cash).unwrap_or(starting_cash),
        });
        index = end;
    }
    out
}

pub fn forecast_cash_flow(req: &CashFlowForecastRequest) -> CashFlowForecastResult {
    let mut daily = Vec::new();
    let mut balance = req.starting_cash;
    let mut min_cash = req.starting_cash;
    let mut peak_needed: f64 = 0.0;

    for day in 0..req.forecast_period_days.max(0) {
        let inflow: f64 = req
            .inflows
            .iter()
            .filter(|i| due(i, day))
            .map(|i| i.amount)
            .sum();
        let outflow: f64 = req
            .outflows
            .iter()
            .filter(|i| due(i, day))
            .map(|i| i.amount)
            .sum();

        balance += inflow - outflow;
        min_cash = min_cash.min(balance);

        daily.push(DailyForecastRow {
            day: day + 1,
            inflow: py_round2(inflow),
            outflow: py_round2(outflow),
            net_flow: py_round2(inflow - outflow),
            ending_cash: py_round2(balance),
        });

        if balance < 0.0 {
            peak_needed = peak_needed.max(balance.abs());
        }
    }

    let weekly = roll_up(&daily, 7, req.starting_cash);
    let monthly = roll_up(&daily, 30, req.starting_cash);

    let total_outflow: f64 = daily.iter().map(|d| d.outflow).sum();
    let burn_rate = if req.forecast_period_days != 0 {
        total_outflow / req.forecast_period_days as f64
    } else {
        0.0
    };
    let runway = if burn_rate > 0.0 {
        Some(req.starting_cash / burn_rate)
    } else {
        None
    };

    let mut scenarios = std::collections::BTreeMap::new();
    scenarios.insert(
        "base".to_string(),
        ScenarioRow {
            ending_cash: balance,
            peak_need: peak_needed,
        },
    );
    // Optimistic and pessimistic are the base scaled, not a re-run, and they
    // are not rounded the way the headline figures are.
    scenarios.insert(
        "optimistic".to_string(),
        ScenarioRow {
            ending_cash: balance * 1.2,
            peak_need: peak_needed * 0.8,
        },
    );
    scenarios.insert(
        "pessimistic".to_string(),
        ScenarioRow {
            ending_cash: balance * 0.7,
            peak_need: peak_needed * 1.3,
        },
    );

    CashFlowForecastResult {
        company_id: req.company_id.clone(),
        period_days: req.forecast_period_days,
        daily_forecasts: daily,
        weekly_summary: weekly,
        monthly_summary: monthly,
        ending_cash: py_round2(balance),
        peak_cash_needed: py_round2(peak_needed),
        min_cash_balance: py_round2(min_cash),
        cash_burn_rate: py_round2(burn_rate),
        runway_days: runway.map(py_round2),
        scenarios,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(amount: f64, frequency: &str) -> CashFlowItem {
        CashFlowItem {
            category: "ops".into(),
            amount,
            frequency: frequency.into(),
            certainty: "high".into(),
        }
    }

    fn request() -> CashFlowForecastRequest {
        CashFlowForecastRequest {
            company_id: "c1".into(),
            starting_cash: 100_000.0,
            inflows: vec![item(5_000.0, "daily"), item(20_000.0, "weekly")],
            outflows: vec![item(6_000.0, "daily"), item(50_000.0, "monthly")],
            forecast_period_days: 10,
            scenario: "base".into(),
        }
    }

    #[test]
    fn monthly_items_are_ignored_and_weekly_lands_on_day_one() {
        let r = forecast_cash_flow(&request());
        assert_eq!(r.daily_forecasts[0].inflow, 25_000.0);
        assert_eq!(r.daily_forecasts[0].outflow, 6_000.0);
        // day 8 is the next weekly landing
        assert_eq!(r.daily_forecasts[7].inflow, 25_000.0);
        assert_eq!(r.daily_forecasts[6].inflow, 5_000.0);
        // the 50k monthly outflow never appears in any day
        assert!(r.daily_forecasts.iter().all(|d| d.outflow == 6_000.0));
    }

    #[test]
    fn rollups_use_the_rounded_daily_figures() {
        let r = forecast_cash_flow(&request());
        let week = &r.weekly_summary[0];
        let summed: f64 = r.daily_forecasts[..7].iter().map(|d| d.inflow).sum();
        assert_eq!(week.total_inflow, summed);
        assert_eq!(week.ending_cash, r.daily_forecasts[6].ending_cash);
        assert_eq!(r.monthly_summary.len(), 1);
        assert_eq!(
            r.monthly_summary[0].ending_cash,
            r.daily_forecasts[9].ending_cash
        );
    }

    #[test]
    fn runway_ignores_inflows_and_is_null_without_burn() {
        let r = forecast_cash_flow(&request());
        assert_eq!(r.cash_burn_rate, 6_000.0);
        assert_eq!(r.runway_days, Some(py_round2(100_000.0 / 6_000.0)));

        let mut idle = request();
        idle.outflows.clear();
        idle.inflows.clear();
        let flat = forecast_cash_flow(&idle);
        assert_eq!(flat.runway_days, None);
        assert_eq!(flat.ending_cash, 100_000.0);
    }

    #[test]
    fn negative_balance_drives_the_peak_need() {
        let mut req = request();
        req.starting_cash = 10_000.0;
        req.inflows.clear();
        let r = forecast_cash_flow(&req);
        assert!(r.peak_cash_needed > 0.0);
        assert_eq!(r.scenarios["base"].peak_need, r.peak_cash_needed);
        assert_eq!(r.scenarios["optimistic"].ending_cash, r.ending_cash * 1.2);
    }
}
