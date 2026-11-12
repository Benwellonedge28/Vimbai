#![forbid(unsafe_code)]
//! Asset allocation, portfolio optimisation and capital rationing, ported from
//! asset-allocation-service, portfolio-optimization-service and
//! capital-allocation-service.
//!
//! Two behaviours are preserved even though they look wrong, because the
//! services shipped them:
//!
//! 1. `tactical_allocation` multiplies the strategic weight by **105** for an
//!    aggressive profile (`v * 105`) while scaling everyone else by `0.95`, so
//!    aggressive tactical figures come out in the thousands of percent. Ported
//!    verbatim; see [`tactical_allocation`].
//! 2. Capital rationing ranks on `npv * 0.6 + strategic_value * 10000 * 0.4`
//!    and takes projects greedily in that order until the budget runs out, so a
//!    cheaper project later in the order is still rejected once budget is thin.
//!    It uses the 8-step IRR ladder from the service, not a root finder.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn r3(value: f64) -> f64 {
    py_round(value, 3)
}

fn r4(value: f64) -> f64 {
    py_round(value, 4)
}

fn r1(value: f64) -> f64 {
    py_round(value, 1)
}

// ---------------------------------------------------------------- asset mix

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetAllocationInput {
    pub company_id: String,
    pub investor_profile: String,
    pub time_horizon: u32,
    pub risk_tolerance: String,
    pub current_portfolio: Vec<(String, f64)>,
    pub total_portfolio_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllocationResult {
    pub asset_class: String,
    pub current_allocation: f64,
    pub target_allocation: f64,
    pub deviation: f64,
    pub rebalance_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetAllocationOutput {
    pub company_id: String,
    pub investor_profile: String,
    pub allocation_results: Vec<AllocationResult>,
    pub total_deviation: f64,
    pub rebalancing_required: bool,
    pub strategic_allocation: Vec<(String, f64)>,
    pub tactical_allocation: Vec<(String, f64)>,
    pub recommendations: Vec<String>,
}

fn target_weights(profile: &str) -> [(&'static str, f64); 4] {
    match profile {
        "conservative" => [
            ("equities", 0.3),
            ("bonds", 0.5),
            ("cash", 0.15),
            ("alternatives", 0.05),
        ],
        "aggressive" => [
            ("equities", 0.7),
            ("bonds", 0.2),
            ("cash", 0.05),
            ("alternatives", 0.05),
        ],
        // moderate and anything unrecognised
        _ => [
            ("equities", 0.5),
            ("bonds", 0.35),
            ("cash", 0.1),
            ("alternatives", 0.05),
        ],
    }
}

/// Tactical weight for a strategic weight. Aggressive profiles scale by 105 as
/// the original service did; the others scale by 0.95.
pub fn tactical_allocation(weight: f64, profile: &str) -> f64 {
    if profile == "aggressive" {
        weight * 105.0
    } else {
        weight * 0.95
    }
}

pub fn allocate_assets(input: &AssetAllocationInput) -> AssetAllocationOutput {
    let targets = target_weights(&input.investor_profile);
    let mut results = Vec::with_capacity(targets.len());
    let mut total_deviation = 0.0;

    for (asset_class, target) in targets {
        let held = input
            .current_portfolio
            .iter()
            .find(|(name, _)| name == asset_class)
            .map(|(_, value)| *value)
            .unwrap_or(0.0);
        let current = if input.total_portfolio_value == 0.0 {
            0.0
        } else {
            held / input.total_portfolio_value
        };
        let deviation = (current - target).abs();
        total_deviation += deviation;
        results.push(AllocationResult {
            asset_class: asset_class.to_string(),
            current_allocation: r2(current * 100.0),
            target_allocation: r2(target * 100.0),
            deviation: r2(deviation * 100.0),
            rebalance_amount: r2((target - current) * input.total_portfolio_value),
        });
    }

    AssetAllocationOutput {
        company_id: input.company_id.clone(),
        investor_profile: input.investor_profile.clone(),
        allocation_results: results,
        total_deviation: r2(total_deviation * 100.0),
        rebalancing_required: total_deviation > 0.05,
        strategic_allocation: targets
            .iter()
            .map(|(name, weight)| (name.to_string(), r2(weight * 100.0)))
            .collect(),
        tactical_allocation: targets
            .iter()
            .map(|(name, weight)| {
                (
                    name.to_string(),
                    r2(tactical_allocation(*weight, &input.investor_profile)),
                )
            })
            .collect(),
        recommendations: vec![
            "Rebalance if deviation exceeds 5%".to_string(),
            "Review allocation quarterly".to_string(),
            "Consider tax implications of rebalancing".to_string(),
        ],
    }
}

// --------------------------------------------------------- portfolio weights

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortfolioAsset {
    pub id: String,
    pub name: String,
    pub market_value: f64,
    pub expected_return: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptimizedAllocation {
    pub asset_id: String,
    pub asset_name: String,
    pub current_weight: f64,
    pub optimal_weight: f64,
    pub target_allocation: f64,
    pub adjustment_needed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontierPoint {
    #[serde(rename = "return")]
    pub expected_return: f64,
    pub risk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortfolioOptimizationOutput {
    pub company_id: String,
    pub optimized_allocations: Vec<OptimizedAllocation>,
    pub expected_return: f64,
    pub expected_risk: f64,
    pub sharpe_ratio: f64,
    pub efficient_frontier: Vec<FrontierPoint>,
    pub recommendations: Vec<String>,
}

/// Equal-weight target, with the service's fixed 15% portfolio risk and constant
/// frontier points.
pub fn optimize_portfolio(
    company_id: &str,
    assets: &[PortfolioAsset],
    risk_free_rate: f64,
) -> PortfolioOptimizationOutput {
    let total_value: f64 = assets.iter().map(|a| a.market_value).sum();
    let count = assets.len();

    let allocations: Vec<OptimizedAllocation> = assets
        .iter()
        .map(|asset| {
            let current_weight = if total_value == 0.0 {
                0.0
            } else {
                asset.market_value / total_value
            };
            let optimal_weight = if count == 0 { 0.0 } else { 1.0 / count as f64 };
            OptimizedAllocation {
                asset_id: asset.id.clone(),
                asset_name: asset.name.clone(),
                current_weight: r4(current_weight),
                optimal_weight: r4(optimal_weight),
                target_allocation: r2(optimal_weight * 100.0),
                adjustment_needed: r2((optimal_weight - current_weight) * total_value),
            }
        })
        .collect();

    let expected_return = if count == 0 {
        0.0
    } else {
        let share = 1.0 / count as f64;
        assets.iter().map(|a| a.expected_return * share).sum()
    };
    let expected_risk = 0.15;
    let sharpe_ratio = if expected_risk == 0.0 {
        0.0
    } else {
        (expected_return - risk_free_rate) / expected_risk
    };

    PortfolioOptimizationOutput {
        company_id: company_id.to_string(),
        optimized_allocations: allocations,
        expected_return: r4(expected_return),
        expected_risk: r4(expected_risk),
        sharpe_ratio: r4(sharpe_ratio),
        efficient_frontier: vec![
            FrontierPoint {
                expected_return: 0.05,
                risk: 0.08,
            },
            FrontierPoint {
                expected_return: 0.08,
                risk: 0.12,
            },
            FrontierPoint {
                expected_return: 0.12,
                risk: 0.18,
            },
            FrontierPoint {
                expected_return: 0.15,
                risk: 0.22,
            },
        ],
        recommendations: vec![
            "Rebalance portfolio to optimal weights".to_string(),
            "Monitor Sharpe ratio regularly".to_string(),
            "Consider risk-adjusted returns".to_string(),
        ],
    }
}

// -------------------------------------------------------- capital rationing

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RationProject {
    pub name: String,
    pub initial_investment: f64,
    pub annual_cash_flows: Vec<f64>,
    pub discount_rate: f64,
    pub strategic_value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredProject {
    pub name: String,
    pub initial_investment: f64,
    pub npv: f64,
    pub irr: f64,
    pub profitability_index: f64,
    pub strategic_value: i32,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapitalAllocationOutput {
    pub company_id: String,
    pub capital_budget: f64,
    pub selected_projects: Vec<ScoredProject>,
    pub rejected_projects: Vec<ScoredProject>,
    pub total_investment: f64,
    pub total_npv: f64,
    pub utilization_pct: f64,
    pub ranking_method: String,
}

fn project_npv(initial: f64, cashflows: &[f64], rate: f64) -> f64 {
    let mut npv = -initial;
    for (index, cashflow) in cashflows.iter().enumerate() {
        npv += cashflow / (1.0 + rate).powi((index + 1) as i32);
    }
    npv
}

/// The service's 8-step IRR ladder: the first rate whose NPV turns
/// non-positive, else 50%.
fn project_irr_ladder(initial: f64, cashflows: &[f64]) -> f64 {
    for rate in [0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.40, 0.50] {
        if project_npv(initial, cashflows, rate) <= 0.0 {
            return rate;
        }
    }
    0.50
}

pub fn allocate_capital(
    company_id: &str,
    capital_budget: f64,
    projects: &[RationProject],
) -> CapitalAllocationOutput {
    let mut scored: Vec<ScoredProject> = projects
        .iter()
        .map(|project| {
            let npv = project_npv(
                project.initial_investment,
                &project.annual_cash_flows,
                project.discount_rate,
            );
            let irr = project_irr_ladder(project.initial_investment, &project.annual_cash_flows);
            let profitability_index = if project.initial_investment == 0.0 {
                0.0
            } else {
                (npv + project.initial_investment) / project.initial_investment
            };
            let score = npv * 0.6 + project.strategic_value as f64 * 10_000.0 * 0.4;
            ScoredProject {
                name: project.name.clone(),
                initial_investment: project.initial_investment,
                npv: r2(npv),
                irr: r1(irr * 100.0),
                profitability_index: r3(profitability_index),
                strategic_value: project.strategic_value,
                score: r2(score),
            }
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut selected = Vec::new();
    let mut rejected = Vec::new();
    let mut invested = 0.0;
    for project in scored {
        if invested + project.initial_investment <= capital_budget {
            invested += project.initial_investment;
            selected.push(project);
        } else {
            rejected.push(project);
        }
    }

    let total_npv: f64 = selected.iter().map(|p| p.npv).sum();
    let utilization = if capital_budget == 0.0 {
        0.0
    } else {
        invested / capital_budget * 100.0
    };

    CapitalAllocationOutput {
        company_id: company_id.to_string(),
        capital_budget: r2(capital_budget),
        selected_projects: selected,
        rejected_projects: rejected,
        total_investment: r2(invested),
        total_npv: r2(total_npv),
        utilization_pct: r1(utilization),
        ranking_method: "NPV_weighted_with_strategic_value".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allocation_input(profile: &str) -> AssetAllocationInput {
        AssetAllocationInput {
            company_id: "c1".into(),
            investor_profile: profile.into(),
            time_horizon: 10,
            risk_tolerance: "medium".into(),
            current_portfolio: vec![
                ("equities".to_string(), 600_000.0),
                ("bonds".to_string(), 300_000.0),
                ("cash".to_string(), 100_000.0),
            ],
            total_portfolio_value: 1_000_000.0,
        }
    }

    #[test]
    fn moderate_profile_deviations() {
        let out = allocate_assets(&allocation_input("moderate"));
        assert_eq!(out.allocation_results.len(), 4);
        assert_eq!(out.allocation_results[0].current_allocation, 60.0);
        assert_eq!(out.allocation_results[0].target_allocation, 50.0);
        assert_eq!(out.allocation_results[0].deviation, 10.0);
        assert_eq!(out.allocation_results[0].rebalance_amount, -100_000.0);
        // equities 10 + bonds 5 + cash 0 + alternatives 5
        assert_eq!(out.total_deviation, 20.0);
        assert!(out.rebalancing_required);
        assert_eq!(out.strategic_allocation[0], ("equities".to_string(), 50.0));
    }

    #[test]
    fn aggressive_tactical_is_scaled_by_105() {
        let out = allocate_assets(&allocation_input("aggressive"));
        assert_eq!(out.total_deviation, 30.0);
        assert_eq!(out.tactical_allocation[0], ("equities".to_string(), 73.5));
        assert_eq!(out.tactical_allocation[2], ("cash".to_string(), 5.25));
        let moderate = allocate_assets(&allocation_input("moderate"));
        assert_eq!(
            moderate.tactical_allocation[0],
            ("equities".to_string(), 0.47)
        );
    }

    #[test]
    fn unknown_profile_falls_back_to_moderate() {
        let odd = allocate_assets(&allocation_input("balanced"));
        let moderate = allocate_assets(&allocation_input("moderate"));
        assert_eq!(odd.strategic_allocation, moderate.strategic_allocation);
    }

    #[test]
    fn zero_value_portfolio_has_no_nan() {
        let mut input = allocation_input("moderate");
        input.total_portfolio_value = 0.0;
        let out = allocate_assets(&input);
        assert_eq!(out.allocation_results[0].current_allocation, 0.0);
        assert_eq!(out.total_deviation, 100.0);
    }

    #[test]
    fn optimisation_equal_weights() {
        let assets = vec![
            PortfolioAsset {
                id: "a".into(),
                name: "Alpha".into(),
                market_value: 600_000.0,
                expected_return: 0.12,
            },
            PortfolioAsset {
                id: "b".into(),
                name: "Beta".into(),
                market_value: 400_000.0,
                expected_return: 0.06,
            },
        ];
        let out = optimize_portfolio("c1", &assets, 0.02);
        assert_eq!(out.optimized_allocations[0].current_weight, 0.6);
        assert_eq!(out.optimized_allocations[0].optimal_weight, 0.5);
        assert_eq!(out.optimized_allocations[0].adjustment_needed, -100_000.0);
        assert_eq!(out.expected_return, 0.09);
        assert_eq!(out.expected_risk, 0.15);
        assert_eq!(out.sharpe_ratio, 0.4667);
        assert_eq!(out.efficient_frontier.len(), 4);
    }

    #[test]
    fn rationing_ranks_and_stops_at_budget() {
        let projects = vec![
            RationProject {
                name: "Dig".into(),
                initial_investment: 400_000.0,
                annual_cash_flows: vec![150_000.0, 150_000.0, 150_000.0, 150_000.0],
                discount_rate: 0.10,
                strategic_value: 8,
            },
            RationProject {
                name: "Thin".into(),
                initial_investment: 500_000.0,
                annual_cash_flows: vec![120_000.0, 120_000.0],
                discount_rate: 0.10,
                strategic_value: 5,
            },
        ];
        let out = allocate_capital("c1", 600_000.0, &projects);
        assert_eq!(out.selected_projects.len(), 1);
        assert_eq!(out.selected_projects[0].name, "Dig");
        assert_eq!(out.rejected_projects[0].name, "Thin");
        assert_eq!(out.total_investment, 400_000.0);
        assert_eq!(out.utilization_pct, 66.7);
        // NPV of Dig straight from the service: 75,479.82
        assert_eq!(out.total_npv, 75_479.82);
        // ladder picks 10% for Dig
        assert_eq!(out.selected_projects[0].irr, 20.0);
        assert_eq!(out.ranking_method, "NPV_weighted_with_strategic_value");
    }
}
