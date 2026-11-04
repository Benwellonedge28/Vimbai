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
use serde_json::Value;
use vimbai_common::Money;

/// Ten is the service's schedule cap, not a natural limit.
const SCHEDULE_CAP: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedAsset {
    pub asset_id: String,
    pub asset_name: String,
    pub category: String,
    pub purchase_date: String,
    pub purchase_cost: Money,
    pub salvage_value: Money,
    pub useful_life_years: i64,
    pub depreciation_method: String,
    pub accumulated_depreciation: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedAssetsRequest {
    pub company_id: String,
    pub assets: Vec<FixedAsset>,
    pub capital_budget: Money,
    pub target_capitalization_rate: Money,
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
fn annual_depreciation(asset: &FixedAsset) -> Money {
    if asset.useful_life_years == 0 {
        Money::zero()
    } else {
        asset
            .purchase_cost
            .minus(&asset.salvage_value)
            .over(&Money::from_i64(asset.useful_life_years))
    }
}

/// A register is a book of amounts: every figure in the report is a sum, a
/// difference or a share of those amounts, so the arithmetic is decimal and only
/// the published values are rounded, once, at two places.
pub fn analyze_fixed_assets(
    request: &FixedAssetsRequest,
) -> Result<FixedAssetsAnalysis, FixedAssetsError> {
    let zero = Money::zero();
    let costs: Vec<Money> = request
        .assets
        .iter()
        .map(|a| a.purchase_cost.clone())
        .collect();
    let total_cost = Money::sum(&costs);
    if total_cost == zero {
        return Err(FixedAssetsError::NoCostInRegister);
    }
    let accumulated: Vec<Money> = request
        .assets
        .iter()
        .map(|a| a.accumulated_depreciation.clone())
        .collect();
    let total_accumulated = Money::sum(&accumulated);

    let depreciation_schedule: Vec<ScheduleRow> = request
        .assets
        .iter()
        .take(SCHEDULE_CAP)
        .map(|asset| {
            let annual = annual_depreciation(asset);
            // `remaining_life` divides accumulated by the annual charge; with
            // no charge the service reports a spent life rather than a guess.
            let remaining = if annual != zero {
                let left = Money::from_i64(asset.useful_life_years)
                    .minus(&asset.accumulated_depreciation.over(&annual));
                if left > zero {
                    left
                } else {
                    zero.clone()
                }
            } else {
                zero.clone()
            };
            ScheduleRow {
                asset_id: asset.asset_id.clone(),
                asset_name: asset.asset_name.clone(),
                net_book_value: asset
                    .purchase_cost
                    .minus(&asset.accumulated_depreciation)
                    .round(2)
                    .to_f64_lossy(),
                remaining_life_years: remaining.round(2).to_f64_lossy(),
                annual_depreciation: annual.round(2).to_f64_lossy(),
            }
        })
        .collect();

    // Insertion order, matching the dict the service builds.
    let mut categories: Vec<(String, Money, Money)> = Vec::new();
    for asset in &request.assets {
        match categories
            .iter_mut()
            .find(|(name, _, _)| *name == asset.category)
        {
            Some(entry) => {
                entry.1 = entry.1.plus(&asset.purchase_cost);
                entry.2 = entry.2.plus(&asset.accumulated_depreciation);
            }
            None => categories.push((
                asset.category.clone(),
                asset.purchase_cost.clone(),
                asset.accumulated_depreciation.clone(),
            )),
        }
    }
    let asset_category_analysis: Vec<CategoryAnalysis> = categories
        .into_iter()
        .map(|(category, cost, accumulated)| CategoryAnalysis {
            category,
            total_cost: cost.round(2).to_f64_lossy(),
            accumulated_dep: accumulated.round(2).to_f64_lossy(),
            net_value: cost.minus(&accumulated).round(2).to_f64_lossy(),
        })
        .collect();

    let tenth = Money::parse("0.1").expect("0.1 is a decimal");
    let three_tenths = Money::parse("0.3").expect("0.3 is a decimal");
    let capital_expenditure_plan = CapitalExpenditurePlan {
        capital_budget: request.capital_budget.to_f64_lossy(),
        replacement_needs: total_cost.times(&tenth).round(2).to_f64_lossy(),
        expansion_allocations: request
            .capital_budget
            .times(&three_tenths)
            .round(2)
            .to_f64_lossy(),
    };

    let mut recommendations = Vec::new();
    let fully_depreciated = request
        .assets
        .iter()
        .filter(|a| a.purchase_cost.minus(&a.accumulated_depreciation) <= a.salvage_value)
        .count();
    if fully_depreciated > 0 {
        recommendations.push(format!(
            "{fully_depreciated} fully depreciated assets need replacement"
        ));
    }
    let renewal_line = Money::parse("0.7").expect("0.7 is a decimal");
    if total_accumulated.over(&total_cost) > renewal_line {
        recommendations.push("High accumulated depreciation - plan for asset renewal".to_string());
    }

    Ok(FixedAssetsAnalysis {
        company_id: request.company_id.clone(),
        asset_summary: AssetSummary {
            total_assets: request.assets.len(),
            total_cost: total_cost.round(2).to_f64_lossy(),
            net_book_value: total_cost.minus(&total_accumulated).round(2).to_f64_lossy(),
        },
        depreciation_schedule,
        asset_category_analysis,
        capital_expenditure_plan,
        recommendations,
    })
}

/// The service's `/analyze` body, out as the service published it.
pub fn analyze_fixed_assets_from_json(payload: &Value) -> Result<Value, String> {
    let request: FixedAssetsRequest =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let report = analyze_fixed_assets(&request).map_err(|e| e.to_string())?;
    serde_json::to_value(&report).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(
        id: &str,
        name: &str,
        category: &str,
        cost: i64,
        salvage: i64,
        life: i64,
        accumulated: i64,
    ) -> FixedAsset {
        FixedAsset {
            asset_id: id.into(),
            asset_name: name.into(),
            category: category.into(),
            purchase_date: "2022-01-01".into(),
            purchase_cost: Money::from_i64(cost),
            salvage_value: Money::from_i64(salvage),
            useful_life_years: life,
            depreciation_method: "straight_line".into(),
            accumulated_depreciation: Money::from_i64(accumulated),
        }
    }

    fn register() -> FixedAssetsRequest {
        FixedAssetsRequest {
            company_id: "c1".into(),
            capital_budget: Money::from_i64(200_000),
            target_capitalization_rate: Money::parse("0.4").expect("0.4 is a decimal"),
            assets: vec![
                asset("a1", "Truck", "vehicle", 60_000, 6_000, 5, 36_000),
                asset("a2", "Rig", "equipment", 30_000, 3_000, 10, 5_400),
                asset("a3", "Old rig", "equipment", 10_000, 1_000, 5, 9_000),
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
        request.assets[0].accumulated_depreciation = Money::from_i64(60_000);
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
                1_000,
                100,
                5,
                200,
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
