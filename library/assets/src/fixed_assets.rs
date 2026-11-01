//! fixed-assets-service: the composite `/analyze` view.
//!
//! `depreciation.rs` already covers the depreciation maths for one asset. What
//! was still Python-only is the register-level report: a book summary, a
//! straight-line schedule for the first ten assets, per-category totals, a
//! blunt capital-expenditure split, and two rules-of-thumb recommendations.
//!
//! Carried over deliberately, because the reports users have filed expect it:
//! category totals keep the order the assets arrive in, the schedule only
//! covers the first ten assets while the summary and categories cover all of
//! them, and replacement needs are a flat 10% of cost rather than anything
//! derived from condition. One deliberate difference: a register with no cost
//! at all returns an error instead of the service's division-by-zero 500.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

/// Ten is the service's schedule cap, not a natural limit.
const SCHEDULE_CAP: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedAsset {
    pub asset_id: String,
    pub asset_name: String,
    pub category: String,
    pub purchase_date: String,
    pub purchase_cost: f64,
    pub salvage_value: f64,
    pub useful_life_years: i64,
    pub depreciation_method: String,
    pub accumulated_depreciation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedAssetsRequest {
    pub company_id: String,
    pub assets: Vec<FixedAsset>,
    pub capital_budget: f64,
    pub target_capitalization_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetSummary {
    pub total_assets: usize,
    pub total_cost: f64,
    pub net_book_value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleRow {
    pub asset_id: String,
    pub asset_name: String,
    pub net_book_value: f64,
    pub remaining_life_years: f64,
    pub annual_depreciation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryAnalysis {
    pub category: String,
    pub total_cost: f64,
    pub accumulated_dep: f64,
    pub net_value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapitalExpenditurePlan {
    pub capital_budget: f64,
    pub replacement_needs: f64,
    pub expansion_allocations: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixedAssetsAnalysis {
    pub company_id: String,
    pub asset_summary: AssetSummary,
    pub depreciation_schedule: Vec<ScheduleRow>,
    pub asset_category_analysis: Vec<CategoryAnalysis>,
    pub capital_expenditure_plan: CapitalExpenditurePlan,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixedAssetsError {
    /// Every figure in the report divides by total cost, so an empty or
    /// zero-valued register has nothing to report.
    NoCostInRegister,
}

impl std::fmt::Display for FixedAssetsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixedAssetsError::NoCostInRegister => {
                write!(f, "register has no asset cost to analyse")
            }
        }
    }
}

/// Straight-line annual charge for one asset, zero when it has no useful life.
fn annual_depreciation(asset: &FixedAsset) -> f64 {
    if asset.useful_life_years == 0 {
        0.0
    } else {
        (asset.purchase_cost - asset.salvage_value) / asset.useful_life_years as f64
    }
}

pub fn analyze_fixed_assets(
    request: &FixedAssetsRequest,
) -> Result<FixedAssetsAnalysis, FixedAssetsError> {
    let total_cost: f64 = request.assets.iter().map(|a| a.purchase_cost).sum();
    if total_cost == 0.0 {
        return Err(FixedAssetsError::NoCostInRegister);
    }
    let total_accumulated: f64 = request
        .assets
        .iter()
        .map(|a| a.accumulated_depreciation)
        .sum();

    let depreciation_schedule: Vec<ScheduleRow> = request
        .assets
        .iter()
        .take(SCHEDULE_CAP)
        .map(|asset| {
            let annual = annual_depreciation(asset);
            // `remaining_life` divides accumulated by the annual charge; with
            // no charge the service reports a spent life rather than a guess.
            let remaining = if annual != 0.0 {
                (asset.useful_life_years as f64 - asset.accumulated_depreciation / annual).max(0.0)
            } else {
                0.0
            };
            ScheduleRow {
                asset_id: asset.asset_id.clone(),
                asset_name: asset.asset_name.clone(),
                net_book_value: py_round2(asset.purchase_cost - asset.accumulated_depreciation),
                remaining_life_years: py_round2(remaining),
                annual_depreciation: py_round2(annual),
            }
        })
        .collect();

    // Insertion order, matching the dict the service builds.
    let mut categories: Vec<(String, f64, f64)> = Vec::new();
    for asset in &request.assets {
        match categories
            .iter_mut()
            .find(|(name, _, _)| *name == asset.category)
        {
            Some(entry) => {
                entry.1 += asset.purchase_cost;
                entry.2 += asset.accumulated_depreciation;
            }
            None => categories.push((
                asset.category.clone(),
                asset.purchase_cost,
                asset.accumulated_depreciation,
            )),
        }
    }
    let asset_category_analysis: Vec<CategoryAnalysis> = categories
        .into_iter()
        .map(|(category, cost, accumulated)| CategoryAnalysis {
            category,
            total_cost: py_round2(cost),
            accumulated_dep: py_round2(accumulated),
            net_value: py_round2(cost - accumulated),
        })
        .collect();

    let capital_expenditure_plan = CapitalExpenditurePlan {
        capital_budget: request.capital_budget,
        replacement_needs: py_round2(total_cost * 0.1),
        expansion_allocations: py_round2(request.capital_budget * 0.3),
    };

    let mut recommendations = Vec::new();
    let fully_depreciated = request
        .assets
        .iter()
        .filter(|a| a.purchase_cost - a.accumulated_depreciation <= a.salvage_value)
        .count();
    if fully_depreciated > 0 {
        recommendations.push(format!(
            "{fully_depreciated} fully depreciated assets need replacement"
        ));
    }
    if total_accumulated / total_cost > 0.7 {
        recommendations.push("High accumulated depreciation - plan for asset renewal".to_string());
    }

    Ok(FixedAssetsAnalysis {
        company_id: request.company_id.clone(),
        asset_summary: AssetSummary {
            total_assets: request.assets.len(),
            total_cost: py_round2(total_cost),
            net_book_value: py_round2(total_cost - total_accumulated),
        },
        depreciation_schedule,
        asset_category_analysis,
        capital_expenditure_plan,
        recommendations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(
        id: &str,
        name: &str,
        category: &str,
        cost: f64,
        salvage: f64,
        life: i64,
        accumulated: f64,
    ) -> FixedAsset {
        FixedAsset {
            asset_id: id.into(),
            asset_name: name.into(),
            category: category.into(),
            purchase_date: "2022-01-01".into(),
            purchase_cost: cost,
            salvage_value: salvage,
            useful_life_years: life,
            depreciation_method: "straight_line".into(),
            accumulated_depreciation: accumulated,
        }
    }

    fn register() -> FixedAssetsRequest {
        FixedAssetsRequest {
            company_id: "c1".into(),
            capital_budget: 200_000.0,
            target_capitalization_rate: 0.4,
            assets: vec![
                asset("a1", "Truck", "vehicle", 60_000.0, 6_000.0, 5, 36_000.0),
                asset("a2", "Rig", "equipment", 30_000.0, 3_000.0, 10, 5_400.0),
                asset("a3", "Old rig", "equipment", 10_000.0, 1_000.0, 5, 9_000.0),
            ],
        }
    }

    #[test]
    fn matches_the_service_reference_report() {
        let report = analyze_fixed_assets(&register()).unwrap();
        assert_eq!(report.asset_summary.total_assets, 3);
        assert_eq!(report.asset_summary.total_cost, 100_000.0);
        assert_eq!(report.asset_summary.net_book_value, 49_600.0);

        let truck = &report.depreciation_schedule[0];
        assert_eq!(truck.annual_depreciation, 10_800.0);
        assert_eq!(truck.remaining_life_years, 1.67);
        assert_eq!(truck.net_book_value, 24_000.0);

        assert_eq!(report.asset_category_analysis.len(), 2);
        assert_eq!(report.asset_category_analysis[0].category, "vehicle");
        assert_eq!(report.asset_category_analysis[1].total_cost, 40_000.0);
        assert_eq!(report.asset_category_analysis[1].accumulated_dep, 14_400.0);
        assert_eq!(report.asset_category_analysis[1].net_value, 25_600.0);

        assert_eq!(report.capital_expenditure_plan.replacement_needs, 10_000.0);
        assert_eq!(
            report.capital_expenditure_plan.expansion_allocations,
            60_000.0
        );
        assert_eq!(
            report.recommendations,
            vec!["1 fully depreciated assets need replacement".to_string()]
        );
    }

    #[test]
    fn a_spent_asset_reports_no_remaining_life() {
        let report = analyze_fixed_assets(&register()).unwrap();
        assert_eq!(report.depreciation_schedule[2].remaining_life_years, 0.0);
    }

    #[test]
    fn high_accumulated_depreciation_adds_the_renewal_advice() {
        let mut request = register();
        request.assets[0].accumulated_depreciation = 60_000.0;
        let report = analyze_fixed_assets(&request).unwrap();
        assert!(report
            .recommendations
            .contains(&"High accumulated depreciation - plan for asset renewal".to_string()));
    }

    #[test]
    fn only_ten_assets_reach_the_schedule() {
        let mut request = register();
        for i in 0..12 {
            request.assets.push(asset(
                &format!("x{i}"),
                "Spare",
                "equipment",
                1_000.0,
                100.0,
                5,
                200.0,
            ));
        }
        let report = analyze_fixed_assets(&request).unwrap();
        assert_eq!(report.depreciation_schedule.len(), 10);
        assert_eq!(report.asset_summary.total_assets, 15);
    }

    #[test]
    fn an_empty_register_is_an_error_not_a_zero_division() {
        let mut request = register();
        request.assets.clear();
        assert_eq!(
            analyze_fixed_assets(&request),
            Err(FixedAssetsError::NoCostInRegister)
        );
    }
}
