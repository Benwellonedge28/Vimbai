#![forbid(unsafe_code)]
//! Cost-Volume-Profit analysis, ported from cvp-analysis-service with
//! exact formula and default-value parity (all analysis fields default
//! to 0.0, matching the pydantic `CVPAnalysis` model).

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CvpAnalysis {
    pub entity_id: String,
    pub entity_name: String,
    pub selling_price_per_unit: f64,
    pub variable_cost_per_unit: f64,
    pub fixed_costs: f64,
    pub expected_sales_units: f64,
    pub target_profit: f64,
    pub contribution_per_unit: f64,
    pub contribution_margin_ratio: f64,
    pub break_even_units: f64,
    pub break_even_revenue: f64,
    pub units_for_target_profit: f64,
    pub revenue_for_target_profit: f64,
    pub expected_sales_revenue: f64,
    pub margin_of_safety_units: f64,
    pub margin_of_safety_revenue: f64,
    pub margin_of_safety_percentage: f64,
    pub profit_at_expected_sales: f64,
    pub degree_of_operating_leverage: f64,
}

/// Full CVP analysis (cvp-analysis-service `_perform_cvp`).
pub fn perform_cvp(
    entity_id: &str,
    entity_name: &str,
    selling_price_per_unit: f64,
    variable_cost_per_unit: f64,
    fixed_costs: f64,
    expected_sales_units: f64,
    target_profit: f64,
) -> CvpAnalysis {
    let mut a = CvpAnalysis {
        entity_id: entity_id.to_string(),
        entity_name: entity_name.to_string(),
        selling_price_per_unit,
        variable_cost_per_unit,
        fixed_costs,
        expected_sales_units,
        target_profit,
        contribution_per_unit: 0.0,
        contribution_margin_ratio: 0.0,
        break_even_units: 0.0,
        break_even_revenue: 0.0,
        units_for_target_profit: 0.0,
        revenue_for_target_profit: 0.0,
        expected_sales_revenue: 0.0,
        margin_of_safety_units: 0.0,
        margin_of_safety_revenue: 0.0,
        margin_of_safety_percentage: 0.0,
        profit_at_expected_sales: 0.0,
        degree_of_operating_leverage: 0.0,
    };

    // Contribution
    a.contribution_per_unit = selling_price_per_unit - variable_cost_per_unit;
    if selling_price_per_unit > 0.0 {
        a.contribution_margin_ratio = a.contribution_per_unit / selling_price_per_unit;
    }

    // Break-even
    if a.contribution_per_unit > 0.0 {
        a.break_even_units = fixed_costs / a.contribution_per_unit;
    }
    a.break_even_revenue = a.break_even_units * selling_price_per_unit;

    // Target profit
    if a.contribution_per_unit > 0.0 {
        a.units_for_target_profit = (fixed_costs + target_profit) / a.contribution_per_unit;
    }
    a.revenue_for_target_profit = a.units_for_target_profit * selling_price_per_unit;

    // Expected sales
    a.expected_sales_revenue = expected_sales_units * selling_price_per_unit;

    // Margin of safety
    if expected_sales_units > 0.0 {
        a.margin_of_safety_units = expected_sales_units - a.break_even_units;
        a.margin_of_safety_revenue = a.margin_of_safety_units * selling_price_per_unit;
        if a.expected_sales_revenue > 0.0 {
            a.margin_of_safety_percentage =
                a.margin_of_safety_revenue / a.expected_sales_revenue * 100.0;
        }
    }

    // Profit at expected sales
    a.profit_at_expected_sales = expected_sales_units * a.contribution_per_unit - fixed_costs;

    // Degree of operating leverage
    if a.profit_at_expected_sales != 0.0 {
        let contribution_total = expected_sales_units * a.contribution_per_unit;
        a.degree_of_operating_leverage = contribution_total / a.profit_at_expected_sales;
    }

    a
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickCvpOutput {
    pub fixed_costs: f64,
    pub selling_price: f64,
    pub variable_cost: f64,
    pub contribution_per_unit: f64,
    pub contribution_margin_ratio: f64,
    pub break_even_units: f64,
    pub break_even_revenue: f64,
}

/// Quick CVP without persistence (cvp-analysis-service `/quick-analysis`).
/// A non-positive contribution yields infinite break-even points.
pub fn quick_cvp(fixed_costs: f64, selling_price: f64, variable_cost: f64) -> QuickCvpOutput {
    let contribution = selling_price - variable_cost;
    let contribution_ratio = if selling_price > 0.0 {
        contribution / selling_price
    } else {
        0.0
    };
    let (be_units, be_revenue) = if contribution > 0.0 {
        (
            fixed_costs / contribution,
            (fixed_costs / contribution) * selling_price,
        )
    } else {
        (f64::INFINITY, f64::INFINITY)
    };
    QuickCvpOutput {
        fixed_costs,
        selling_price,
        variable_cost,
        contribution_per_unit: py_round(contribution, 4),
        contribution_margin_ratio: py_round(contribution_ratio, 4),
        break_even_units: py_round2(be_units),
        break_even_revenue: py_round2(be_revenue),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionOutput {
    pub entity_id: String,
    pub entity_name: String,
    pub selling_price_per_unit: f64,
    pub variable_cost_per_unit: f64,
    pub contribution_per_unit: f64,
    pub contribution_margin_ratio: f64,
    pub contribution_margin_percentage: f64,
    pub units_sold: f64,
    pub total_contribution: f64,
}

/// Contribution margin (cvp-analysis-service `/contribution`).
pub fn contribution(
    entity_id: &str,
    entity_name: &str,
    selling_price_per_unit: f64,
    variable_cost_per_unit: f64,
    units_sold: f64,
) -> ContributionOutput {
    let contribution_per_unit = selling_price_per_unit - variable_cost_per_unit;
    let contribution_margin_ratio = if selling_price_per_unit > 0.0 {
        contribution_per_unit / selling_price_per_unit
    } else {
        0.0
    };
    let total_contribution = contribution_per_unit * units_sold;
    ContributionOutput {
        entity_id: entity_id.to_string(),
        entity_name: entity_name.to_string(),
        selling_price_per_unit,
        variable_cost_per_unit,
        contribution_per_unit: py_round(contribution_per_unit, 4),
        contribution_margin_ratio: py_round(contribution_margin_ratio, 4),
        contribution_margin_percentage: py_round(contribution_margin_ratio * 100.0, 2),
        units_sold,
        total_contribution: py_round2(total_contribution),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetProfitOutput {
    pub entity_name: String,
    pub target_profit: f64,
    pub contribution_per_unit: f64,
    pub contribution_margin_ratio: f64,
    pub required_units: f64,
    pub required_revenue: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired_return_on_sales_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_revenue_for_ros: Option<f64>,
}

/// Target profit units/revenue (cvp-analysis-service `/target-profit`).
/// Mirrors the Python 400 on non-positive contribution as an `Err`.
pub fn target_profit(
    entity_name: &str,
    selling_price_per_unit: f64,
    variable_cost_per_unit: f64,
    fixed_costs: f64,
    target_profit: f64,
    desired_return_on_sales: Option<f64>,
) -> Result<TargetProfitOutput, String> {
    let contribution = selling_price_per_unit - variable_cost_per_unit;
    let cm_ratio = if selling_price_per_unit > 0.0 {
        contribution / selling_price_per_unit
    } else {
        0.0
    };
    if contribution <= 0.0 {
        return Err("Contribution per unit must be positive.".to_string());
    }
    let required_units = (fixed_costs + target_profit) / contribution;
    let required_revenue = required_units * selling_price_per_unit;

    let mut out = TargetProfitOutput {
        entity_name: entity_name.to_string(),
        target_profit,
        contribution_per_unit: py_round(contribution, 4),
        contribution_margin_ratio: py_round(cm_ratio, 4),
        required_units: py_round2(required_units),
        required_revenue: py_round2(required_revenue),
        desired_return_on_sales_pct: None,
        required_revenue_for_ros: None,
    };
    if let Some(ros) = desired_return_on_sales {
        if ros > 0.0 {
            out.desired_return_on_sales_pct = Some(ros);
            out.required_revenue_for_ros = Some(py_round2(target_profit / (ros / 100.0)));
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiProductItem {
    pub product_name: String,
    pub selling_price: f64,
    pub variable_cost: f64,
    pub expected_proportion: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductBreakdown {
    pub product_name: String,
    pub selling_price: f64,
    pub variable_cost: f64,
    pub contribution_per_unit: f64,
    pub contribution_margin_ratio: f64,
    pub expected_proportion_pct: f64,
    pub break_even_revenue_share: f64,
    pub target_revenue_share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiProductOutput {
    pub entity_id: String,
    pub entity_name: String,
    pub fixed_costs: f64,
    pub target_profit: f64,
    pub weighted_contribution_margin_ratio: f64,
    pub break_even_revenue: f64,
    pub target_revenue: f64,
    pub product_breakdown: Vec<ProductBreakdown>,
}

/// Multi-product break-even with a sales mix
/// (cvp-analysis-service `/multi-product`).
pub fn multi_product_cvp(
    entity_id: &str,
    entity_name: &str,
    fixed_costs: f64,
    target_profit: f64,
    products: &[MultiProductItem],
) -> Result<MultiProductOutput, String> {
    if products.is_empty() {
        return Err("At least one product is required.".to_string());
    }

    let mut weighted_cm_ratio = 0.0;
    let mut details = Vec::with_capacity(products.len());
    for prod in products {
        let cm = prod.selling_price - prod.variable_cost;
        let cm_ratio = if prod.selling_price > 0.0 {
            cm / prod.selling_price
        } else {
            0.0
        };
        weighted_cm_ratio += cm_ratio * (prod.expected_proportion / 100.0);
        details.push(ProductBreakdown {
            product_name: prod.product_name.clone(),
            selling_price: prod.selling_price,
            variable_cost: prod.variable_cost,
            contribution_per_unit: py_round(cm, 4),
            contribution_margin_ratio: py_round(cm_ratio, 4),
            expected_proportion_pct: prod.expected_proportion,
            break_even_revenue_share: 0.0,
            target_revenue_share: 0.0,
        });
    }

    let break_even_revenue = if weighted_cm_ratio > 0.0 {
        fixed_costs / weighted_cm_ratio
    } else {
        f64::INFINITY
    };
    let target_revenue = if weighted_cm_ratio > 0.0 {
        (fixed_costs + target_profit) / weighted_cm_ratio
    } else {
        f64::INFINITY
    };

    for d in details.iter_mut() {
        let proportion = d.expected_proportion_pct / 100.0;
        d.break_even_revenue_share = py_round2(break_even_revenue * proportion);
        d.target_revenue_share = py_round2(target_revenue * proportion);
    }

    Ok(MultiProductOutput {
        entity_id: entity_id.to_string(),
        entity_name: entity_name.to_string(),
        fixed_costs,
        target_profit,
        weighted_contribution_margin_ratio: py_round(weighted_cm_ratio, 4),
        break_even_revenue: py_round2(break_even_revenue),
        target_revenue: py_round2(target_revenue),
        product_breakdown: details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perform_cvp_matches_python() {
        // SP=20 VC=12 FC=8000, expected 1200 units, target 3000
        let a = perform_cvp("e1", "Book A", 20.0, 12.0, 8000.0, 1200.0, 3000.0);
        assert_eq!(a.contribution_per_unit, 8.0);
        assert_eq!(a.contribution_margin_ratio, 0.4);
        assert_eq!(a.break_even_units, 1000.0);
        assert_eq!(a.break_even_revenue, 20000.0);
        assert_eq!(a.units_for_target_profit, (8000.0 + 3000.0) / 8.0);
        assert_eq!(a.expected_sales_revenue, 24000.0);
        assert_eq!(a.margin_of_safety_units, 200.0);
        assert_eq!(a.margin_of_safety_revenue, 4000.0);
        assert_eq!(a.margin_of_safety_percentage, 4000.0 / 24000.0 * 100.0);
        assert_eq!(a.profit_at_expected_sales, 1200.0 * 8.0 - 8000.0);
        assert_eq!(a.degree_of_operating_leverage, 9600.0 / 1600.0);
    }

    #[test]
    fn perform_cvp_zero_contribution_keeps_zero_defaults() {
        let a = perform_cvp("e1", "X", 10.0, 10.0, 100.0, 50.0, 0.0);
        assert_eq!(a.break_even_units, 0.0);
        assert_eq!(a.units_for_target_profit, 0.0);
        assert_eq!(a.profit_at_expected_sales, -100.0);
    }

    #[test]
    fn quick_cvp_matches_python() {
        let q = quick_cvp(5000.0, 25.0, 15.0);
        assert_eq!(q.contribution_per_unit, 10.0);
        assert_eq!(q.contribution_margin_ratio, 0.4);
        assert_eq!(q.break_even_units, 500.0);
        assert_eq!(q.break_even_revenue, 12500.0);
    }

    #[test]
    fn quick_cvp_infinite_when_no_contribution() {
        let q = quick_cvp(100.0, 10.0, 15.0);
        assert!(q.break_even_units.is_infinite());
    }

    #[test]
    fn contribution_endpoint_parity() {
        let c = contribution("e1", "X", 20.0, 12.0, 350.0);
        assert_eq!(c.contribution_per_unit, 8.0);
        assert_eq!(c.contribution_margin_percentage, 40.0);
        assert_eq!(c.total_contribution, 2800.0);
    }

    #[test]
    fn target_profit_includes_ros() {
        let r = target_profit("X", 20.0, 12.0, 8000.0, 3000.0, Some(10.0)).unwrap();
        assert_eq!(r.required_units, 1375.0);
        assert_eq!(r.required_revenue, 27500.0);
        assert_eq!(r.required_revenue_for_ros, Some(30000.0));
        assert!(target_profit("X", 10.0, 12.0, 0.0, 0.0, None).is_err());
    }

    #[test]
    fn multi_product_mix() {
        let products = vec![
            MultiProductItem {
                product_name: "A".into(),
                selling_price: 10.0,
                variable_cost: 6.0,
                expected_proportion: 60.0,
            },
            MultiProductItem {
                product_name: "B".into(),
                selling_price: 20.0,
                variable_cost: 10.0,
                expected_proportion: 40.0,
            },
        ];
        let m = multi_product_cvp("e1", "X", 50000.0, 10000.0, &products).unwrap();
        // weighted cm ratio = 0.4*0.6 + 0.5*0.4 = 0.44
        assert_eq!(m.weighted_contribution_margin_ratio, 0.44);
        assert_eq!(m.break_even_revenue, 113636.36);
        assert_eq!(m.target_revenue, 136363.64);
        assert_eq!(
            m.product_breakdown[0].break_even_revenue_share,
            py_round2(50000.0 / 0.44 * 0.6)
        );
        assert!(multi_product_cvp("e1", "X", 1.0, 0.0, &[]).is_err());
    }
}
