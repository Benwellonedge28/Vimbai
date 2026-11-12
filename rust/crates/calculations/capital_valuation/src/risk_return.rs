#![forbid(unsafe_code)]
//! Risk-adjusted return metrics, ported from risk-return-analysis-service.
//!
//! The service computed population statistics (divide by n, not n-1), and that
//! is kept. Three of its quirks are pinned rather than corrected:
//!
//! * Sortino uses the sum of squared *negative* returns over their own count. If
//!   no return is negative the downside variance falls back to the full
//!   variance, so the ratio stays defined.
//! * Portfolio risk is the mean standard deviation times 0.8, not a covariance
//!   matrix. It is a fixed haircut, not diversification.
//! * The efficient frontier is four hard-coded points dating from the original
//!   service; nothing is optimised.
//!
//! `information_ratio` is alpha divided by a constant 0.1.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn r4(value: f64) -> f64 {
    py_round(value, 4)
}

fn r6(value: f64) -> f64 {
    py_round(value, 6)
}

fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetInput {
    pub id: String,
    pub name: String,
    pub expected_return: f64,
    pub beta: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskReturnRequest {
    pub company_id: String,
    pub assets: Vec<AssetInput>,
    pub benchmark_return: f64,
    pub risk_free_rate: f64,
    #[serde(default)]
    pub returns_data: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskReturnMetrics {
    pub asset_id: String,
    pub asset_name: String,
    pub expected_return: f64,
    pub standard_deviation: f64,
    pub variance: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub treynor_ratio: f64,
    pub beta: f64,
    pub alpha: f64,
    pub information_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskReturnFrontierPoint {
    pub expected_return: f64,
    pub risk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskReturnOutput {
    pub company_id: String,
    pub risk_return_metrics: Vec<RiskReturnMetrics>,
    pub portfolio_expected_return: f64,
    pub portfolio_risk: f64,
    pub portfolio_sharpe_ratio: f64,
    pub efficient_frontier: Vec<RiskReturnFrontierPoint>,
    pub recommendations: Vec<String>,
}

pub fn analyze_risk_return(request: &RiskReturnRequest) -> RiskReturnOutput {
    let mut metrics = Vec::with_capacity(request.assets.len());

    for (index, asset) in request.assets.iter().enumerate() {
        let returns: &[f64] = request
            .returns_data
            .get(index)
            .map(|row| row.as_slice())
            .unwrap_or(&[]);

        // Empty return series falls back to the asset's own expected return and
        // a 2% variance, exactly as the service did.
        let expected = if returns.is_empty() {
            asset.expected_return
        } else {
            returns.iter().sum::<f64>() / returns.len() as f64
        };

        let variance = if returns.is_empty() {
            0.02
        } else {
            returns.iter().map(|r| (r - expected).powi(2)).sum::<f64>() / returns.len() as f64
        };

        let std_dev = variance.sqrt();

        let downside: Vec<f64> = returns.iter().copied().filter(|r| *r < 0.0).collect();
        let downside_variance = if downside.is_empty() {
            variance
        } else {
            downside.iter().map(|r| r * r).sum::<f64>() / downside.len() as f64
        };

        let excess = expected - request.risk_free_rate;
        let sortino = safe_div(excess, downside_variance.sqrt());
        let sharpe = safe_div(excess, std_dev);
        let beta = asset.beta;
        let treynor = safe_div(excess, beta);
        let alpha = expected
            - (request.risk_free_rate + beta * (request.benchmark_return - request.risk_free_rate));

        metrics.push(RiskReturnMetrics {
            asset_id: asset.id.clone(),
            asset_name: asset.name.clone(),
            expected_return: r4(expected),
            standard_deviation: r4(std_dev),
            variance: r6(variance),
            sharpe_ratio: r4(sharpe),
            sortino_ratio: r4(sortino),
            treynor_ratio: r4(treynor),
            beta: r4(beta),
            alpha: r4(alpha),
            information_ratio: r4(alpha / 0.1),
        });
    }

    let count = metrics.len() as f64;
    let portfolio_expected = metrics.iter().map(|m| m.expected_return).sum::<f64>()
        / if count == 0.0 { 1.0 } else { count };
    let portfolio_risk = metrics.iter().map(|m| m.standard_deviation).sum::<f64>()
        / if count == 0.0 { 1.0 } else { count }
        * 0.8;
    let portfolio_sharpe = safe_div(portfolio_expected - request.risk_free_rate, portfolio_risk);

    RiskReturnOutput {
        company_id: request.company_id.clone(),
        risk_return_metrics: metrics,
        portfolio_expected_return: r4(portfolio_expected),
        portfolio_risk: r4(portfolio_risk),
        portfolio_sharpe_ratio: r4(portfolio_sharpe),
        efficient_frontier: vec![
            RiskReturnFrontierPoint {
                expected_return: 0.05,
                risk: 0.05,
            },
            RiskReturnFrontierPoint {
                expected_return: 0.08,
                risk: 0.08,
            },
            RiskReturnFrontierPoint {
                expected_return: 0.12,
                risk: 0.12,
            },
            RiskReturnFrontierPoint {
                expected_return: 0.15,
                risk: 0.16,
            },
        ],
        recommendations: vec![
            "Consider assets with higher Sharpe ratios".to_string(),
            "Monitor Sortino ratio for downside risk".to_string(),
            "Review beta against market movements".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> RiskReturnRequest {
        RiskReturnRequest {
            company_id: "C1".to_string(),
            assets: vec![
                AssetInput {
                    id: "A".to_string(),
                    name: "Alpha".to_string(),
                    expected_return: 0.1,
                    beta: 1.2,
                },
                AssetInput {
                    id: "B".to_string(),
                    name: "Beta".to_string(),
                    expected_return: 0.08,
                    beta: 0.8,
                },
            ],
            benchmark_return: 0.09,
            risk_free_rate: 0.03,
            returns_data: vec![vec![0.05, -0.02, 0.08, 0.01], vec![-0.01, 0.02, 0.03, 0.04]],
        }
    }

    #[test]
    fn matches_the_service_metrics() {
        let out = analyze_risk_return(&request());
        let a = &out.risk_return_metrics[0];
        assert_eq!(a.asset_id, "A");
        assert_eq!(a.expected_return, 0.03);
        assert_eq!(a.standard_deviation, 0.0381);
        assert_eq!(a.variance, 0.00145);
        assert_eq!(a.sharpe_ratio, 0.0);
        assert_eq!(a.sortino_ratio, 0.0);
        assert_eq!(a.treynor_ratio, 0.0);
        assert_eq!(a.alpha, -0.072);
        assert_eq!(a.information_ratio, -0.72);

        let b = &out.risk_return_metrics[1];
        assert_eq!(b.expected_return, 0.02);
        assert_eq!(b.standard_deviation, 0.0187);
        assert_eq!(b.sharpe_ratio, -0.5345);
        assert_eq!(b.sortino_ratio, -1.0);
        assert_eq!(b.treynor_ratio, -0.0125);
    }

    #[test]
    fn portfolio_risk_is_the_mean_volatility_haircut() {
        let out = analyze_risk_return(&request());
        assert_eq!(out.portfolio_expected_return, 0.025);
        // (0.0381 + 0.0187) / 2 * 0.8
        assert_eq!(out.portfolio_risk, 0.0227);
        assert_eq!(out.portfolio_sharpe_ratio, -0.2201);
        assert_eq!(out.efficient_frontier.len(), 4);
        assert_eq!(out.efficient_frontier[3].risk, 0.16);
    }

    #[test]
    fn empty_returns_fall_back_to_the_asset_expectation() {
        let mut req = request();
        req.returns_data = vec![];
        let out = analyze_risk_return(&req);
        assert_eq!(out.risk_return_metrics[0].expected_return, 0.1);
        assert_eq!(out.risk_return_metrics[0].variance, 0.02);
    }
}
