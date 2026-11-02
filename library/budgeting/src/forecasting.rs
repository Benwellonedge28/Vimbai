//! financial-forecasting-service: project a metric forward from its history.
//!
//! Three paths. Fewer than three observations gives a naive repeat of the last
//! value. `auto` fits a least-squares line and only uses it when R² clears 0.7,
//! falling back to exponential smoothing (alpha 0.3). Both paths band the
//! forecast at 1.96 standard errors.
//!
//! PYTHON QUIRK: the exponential branch extrapolates from a percentage change
//! between the first and last smoothed value, spread evenly across the horizon,
//! so its growth rate and its forecast are two different pictures of the same
//! history. Preserved, band and all.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

const ALPHA: f64 = 0.3;
const Z: f64 = 1.96;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForecastRequest {
    #[serde(default)]
    pub company_id: String,
    #[serde(default)]
    pub metric_name: String,
    #[serde(default)]
    pub historical_values: Vec<f64>,
    #[serde(default = "default_periods")]
    pub periods_ahead: usize,
    #[serde(default = "default_method")]
    pub method: String,
}

fn default_periods() -> usize {
    4
}

fn default_method() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ForecastResult {
    pub id: String,
    pub company_id: String,
    pub metric_name: String,
    pub forecast: Vec<f64>,
    pub method_used: String,
    pub confidence_interval_lower: Vec<f64>,
    pub confidence_interval_upper: Vec<f64>,
    pub trend: String,
    pub growth_rate: f64,
    pub r_squared: f64,
}

/// Least-squares fit against the index, returning slope, intercept and R².
fn linear_regression(values: &[f64]) -> (f64, f64, f64) {
    let n = values.len();
    if n < 2 {
        return (0.0, values.last().copied().unwrap_or(0.0), 0.0);
    }
    let nf = n as f64;
    let x_sum: f64 = (0..n).map(|i| i as f64).sum();
    let y_sum: f64 = values.iter().sum();
    let xy_sum: f64 = values.iter().enumerate().map(|(i, v)| i as f64 * v).sum();
    let x2_sum: f64 = (0..n).map(|i| (i * i) as f64).sum();
    let denom = nf * x2_sum - x_sum * x_sum;
    if denom == 0.0 {
        return (0.0, y_sum / nf, 0.0);
    }
    let slope = (nf * xy_sum - x_sum * y_sum) / denom;
    let intercept = (y_sum - slope * x_sum) / nf;
    let ss_res: f64 = values
        .iter()
        .enumerate()
        .map(|(i, v)| (v - (slope * i as f64 + intercept)).powi(2))
        .sum();
    let mean = y_sum / nf;
    let ss_tot: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    let r2 = if ss_tot != 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };
    (slope, intercept, r2)
}

fn exponential_smoothing(values: &[f64]) -> Vec<f64> {
    let mut result: Vec<f64> = Vec::with_capacity(values.len());
    for (i, v) in values.iter().enumerate() {
        if i == 0 {
            result.push(*v);
        } else {
            let prev = result[i - 1];
            result.push(ALPHA * v + (1.0 - ALPHA) * prev);
        }
    }
    result
}

fn trend_of(from: f64, to: f64) -> String {
    if to > from {
        "increasing".to_string()
    } else if to < from {
        "decreasing".to_string()
    } else {
        "stable".to_string()
    }
}

pub fn forecast_metric(req: &ForecastRequest, report_id: &str) -> ForecastResult {
    let values = &req.historical_values;
    let n = values.len();

    let (forecast, method_used, lower, upper, trend, growth_rate, r2) = if n < 3 {
        let repeat = values.last().copied().unwrap_or(0.0);
        (
            vec![repeat; req.periods_ahead],
            "naive".to_string(),
            vec![0.0; req.periods_ahead],
            vec![0.0; req.periods_ahead],
            "insufficient_data".to_string(),
            0.0,
            0.0,
        )
    } else {
        // the fit is run twice in the service: once to choose the method, once to use it
        let (_, _, r2_probe) = linear_regression(values);
        let method = if req.method == "auto" || req.method.is_empty() {
            if r2_probe > 0.7 {
                "linear".to_string()
            } else {
                "exponential".to_string()
            }
        } else {
            req.method.clone()
        };

        if method == "linear" {
            let (slope, intercept, r2) = linear_regression(values);
            let forecast: Vec<f64> = (0..req.periods_ahead)
                .map(|i| slope * (n + i) as f64 + intercept)
                .collect();
            let growth_rate = if intercept != 0.0 {
                slope / intercept * 100.0
            } else {
                0.0
            };
            let trend = trend_of(0.0, slope);
            let ss: f64 = values
                .iter()
                .enumerate()
                .map(|(i, v)| (v - (slope * i as f64 + intercept)).powi(2))
                .sum();
            let std_err = (ss / (n.saturating_sub(2)).max(1) as f64).sqrt();
            let lower = forecast.iter().map(|f| f - Z * std_err).collect();
            let upper = forecast.iter().map(|f| f + Z * std_err).collect();
            (forecast, method, lower, upper, trend, growth_rate, r2)
        } else {
            let smoothed = exponential_smoothing(values);
            let last = smoothed[smoothed.len() - 1];
            let first = smoothed[0];
            let pct = (last - first) / first.abs().max(1.0);
            let forecast: Vec<f64> = (0..req.periods_ahead)
                .map(|i| last * (1.0 + pct * (i as f64 + 1.0) / n as f64))
                .collect();
            let growth_rate = if first != 0.0 {
                (last - first) / first * 100.0
            } else {
                0.0
            };
            let trend = trend_of(first, last);
            let ss: f64 = values
                .iter()
                .zip(smoothed.iter())
                .map(|(v, s)| (v - s).powi(2))
                .sum();
            let std_err = (ss / (n.saturating_sub(1)).max(1) as f64).sqrt();
            let lower = forecast.iter().map(|f| f - Z * std_err).collect();
            let upper = forecast.iter().map(|f| f + Z * std_err).collect();
            (forecast, method, lower, upper, trend, growth_rate, 0.0)
        }
    };

    ForecastResult {
        id: report_id.to_string(),
        company_id: req.company_id.clone(),
        metric_name: req.metric_name.clone(),
        forecast: forecast.iter().map(|f| py_round(*f, 2)).collect(),
        method_used,
        confidence_interval_lower: lower.iter().map(|f| py_round(*f, 2)).collect(),
        confidence_interval_upper: upper.iter().map(|f| py_round(*f, 2)).collect(),
        trend,
        growth_rate: py_round(growth_rate, 2),
        r_squared: py_round(r2, 4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(values: Vec<f64>, method: &str) -> ForecastRequest {
        ForecastRequest {
            company_id: "c1".to_string(),
            metric_name: "revenue".to_string(),
            historical_values: values,
            periods_ahead: 2,
            method: method.to_string(),
        }
    }

    #[test]
    fn short_histories_repeat_the_last_value() {
        let result = forecast_metric(&request(vec![10.0, 12.0], "auto"), "id");
        assert_eq!(result.method_used, "naive");
        assert_eq!(result.forecast, vec![12.0, 12.0]);
        assert_eq!(result.confidence_interval_lower, vec![0.0, 0.0]);
        assert_eq!(result.trend, "insufficient_data");
        assert_eq!(result.r_squared, 0.0);
    }

    #[test]
    fn a_clean_line_is_fitted_with_its_r_squared() {
        // a perfect straight line makes R² one, so auto picks the line
        let result = forecast_metric(&request(vec![10.0, 20.0, 30.0, 40.0], "auto"), "id");
        assert_eq!(result.method_used, "linear");
        assert_eq!(result.r_squared, 1.0);
        assert_eq!(result.forecast, vec![50.0, 60.0]);
        assert_eq!(result.trend, "increasing");
        // slope over intercept: 10 / 10 * 100
        assert_eq!(result.growth_rate, 100.0);
    }

    #[test]
    fn a_noisy_history_falls_back_to_smoothing() {
        let result = forecast_metric(&request(vec![10.0, 40.0, 15.0, 45.0], "auto"), "id");
        assert_eq!(result.method_used, "exponential");
        // the smoothed series is 10, 19, 17.8, 25.96, and the 159.6% gap between
        // its ends is spread over the horizon
        assert_eq!(result.forecast, vec![36.32, 46.68]);
        assert_eq!(result.confidence_interval_lower, vec![4.08, 14.44]);
        assert_eq!(result.confidence_interval_upper, vec![68.55, 78.91]);
        assert_eq!(result.growth_rate, 159.6);
        assert_eq!(result.trend, "increasing");
    }

    #[test]
    fn an_explicit_method_is_honoured() {
        let result = forecast_metric(&request(vec![10.0, 20.0, 30.0], "linear"), "id");
        assert_eq!(result.method_used, "linear");
        let smoothed = forecast_metric(&request(vec![10.0, 20.0, 30.0], "exponential"), "id");
        assert_eq!(smoothed.method_used, "exponential");
    }

    #[test]
    fn a_flat_history_is_stable_with_a_zero_band() {
        let result = forecast_metric(&request(vec![5.0, 5.0, 5.0], "linear"), "id");
        assert_eq!(result.trend, "stable");
        assert_eq!(result.forecast, vec![5.0, 5.0]);
        assert_eq!(result.growth_rate, 0.0);
        assert_eq!(result.confidence_interval_upper, vec![5.0, 5.0]);
    }

    #[test]
    fn a_declining_line_reads_decreasing() {
        let result = forecast_metric(&request(vec![40.0, 30.0, 20.0, 10.0], "linear"), "id");
        assert_eq!(result.trend, "decreasing");
        assert_eq!(result.forecast, vec![0.0, -10.0]);
        assert_eq!(result.growth_rate, -25.0);
    }
}
