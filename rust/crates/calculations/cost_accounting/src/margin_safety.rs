#![forbid(unsafe_code)]
//! Margin of safety, ported from margin-safety-service.
//!
//! The Python service keeps a list of calculations; only the arithmetic is
//! mirrored here. Two quirks are preserved deliberately: a
//! target of 100% or more yields an infinite required sales figure rather
//! than an error, and the risk bands are strictly `>= 20` / `>= 10`, so
//! exactly 20% is "low" and exactly 10% is "medium".

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarginOfSafetyOutput {
    pub current_output: f64,
    pub current_revenue: f64,
    pub break_even_output: f64,
    pub break_even_revenue: f64,
    pub margin_of_safety_units: f64,
    pub margin_of_safety_revenue: f64,
    pub margin_of_safety_percentage: f64,
    pub margin_of_safety_ratio: f64,
    pub risk_level: String,
}

fn risk_level(percentage: f64) -> String {
    if percentage >= 20.0 {
        "low".to_string()
    } else if percentage >= 10.0 {
        "medium".to_string()
    } else {
        "high".to_string()
    }
}

/// `/calculate`: units and revenue margins against stated break-even
/// figures. Ratios stay at zero when the denominator is not positive.
pub fn margin_of_safety(
    current_output: f64,
    current_revenue: f64,
    break_even_output: f64,
    break_even_revenue: f64,
) -> MarginOfSafetyOutput {
    let units = current_output - break_even_output;
    let revenue = current_revenue - break_even_revenue;
    let percentage = if current_revenue > 0.0 {
        revenue / current_revenue * 100.0
    } else {
        0.0
    };
    let ratio = if current_output > 0.0 {
        units / current_output
    } else {
        0.0
    };
    MarginOfSafetyOutput {
        current_output,
        current_revenue,
        break_even_output,
        break_even_revenue,
        margin_of_safety_units: units,
        margin_of_safety_revenue: revenue,
        margin_of_safety_percentage: percentage,
        margin_of_safety_ratio: ratio,
        risk_level: risk_level(percentage),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarginFromDataOutput {
    pub current_sales_units: f64,
    pub current_revenue: f64,
    pub break_even_units: f64,
    pub break_even_revenue: f64,
    pub margin_of_safety_units: f64,
    pub margin_of_safety_revenue: f64,
    pub margin_of_safety_percentage: f64,
    pub risk_level: String,
}

/// `/calculate-from-data`: derives break-even from price and cost. A
/// non-positive contribution margin gives break-even units of 0 instead of
/// dividing by zero.
pub fn margin_from_basic_data(
    selling_price: f64,
    fixed_costs: f64,
    variable_cost_per_unit: f64,
    expected_sales_units: f64,
) -> MarginFromDataOutput {
    let contribution = selling_price - variable_cost_per_unit;
    let break_even_units = if contribution > 0.0 {
        fixed_costs / contribution
    } else {
        0.0
    };
    let break_even_revenue = break_even_units * selling_price;
    let current_revenue = expected_sales_units * selling_price;
    let mos_units = expected_sales_units - break_even_units;
    let mos_revenue = current_revenue - break_even_revenue;
    let percentage = if current_revenue > 0.0 {
        mos_revenue / current_revenue * 100.0
    } else {
        0.0
    };
    MarginFromDataOutput {
        current_sales_units: expected_sales_units,
        current_revenue,
        break_even_units,
        break_even_revenue,
        margin_of_safety_units: mos_units,
        margin_of_safety_revenue: mos_revenue,
        margin_of_safety_percentage: percentage,
        risk_level: risk_level(percentage),
    }
}

/// `/target-mos`: sales needed for a target margin. A target of 100% or
/// more returns infinity, exactly as the Python does.
pub fn sales_for_target_mos(break_even_revenue: f64, target_mos_percentage: f64) -> f64 {
    if target_mos_percentage < 100.0 {
        break_even_revenue / (1.0 - target_mos_percentage / 100.0)
    } else {
        f64::INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_revenue_margins() {
        let out = margin_of_safety(2000.0, 100_000.0, 1500.0, 75_000.0);
        assert_eq!(out.margin_of_safety_units, 500.0);
        assert_eq!(out.margin_of_safety_revenue, 25_000.0);
        assert_eq!(out.margin_of_safety_percentage, 25.0);
        assert_eq!(out.margin_of_safety_ratio, 0.25);
        assert_eq!(out.risk_level, "low");
    }

    #[test]
    fn risk_bands_are_inclusive() {
        assert_eq!(margin_of_safety(20.0, 100.0, 0.0, 80.0).risk_level, "low");
        assert_eq!(
            margin_of_safety(10.0, 100.0, 0.0, 90.0).risk_level,
            "medium"
        );
        assert_eq!(margin_of_safety(1.0, 100.0, 0.0, 99.0).risk_level, "high");
    }

    #[test]
    fn zero_revenue_keeps_ratio_quirks() {
        let out = margin_of_safety(0.0, 0.0, 0.0, 0.0);
        assert_eq!(out.margin_of_safety_percentage, 0.0);
        assert_eq!(out.margin_of_safety_ratio, 0.0);
        assert_eq!(out.risk_level, "high");
    }

    #[test]
    fn break_even_from_basic_data() {
        let out = margin_from_basic_data(50.0, 30_000.0, 30.0, 2000.0);
        assert_eq!(out.break_even_units, 1500.0);
        assert_eq!(out.break_even_revenue, 75_000.0);
        assert_eq!(out.current_revenue, 100_000.0);
        assert_eq!(out.margin_of_safety_percentage, 25.0);
        assert_eq!(out.risk_level, "low");
    }

    #[test]
    fn zero_contribution_avoids_division() {
        let out = margin_from_basic_data(30.0, 30_000.0, 30.0, 2000.0);
        assert_eq!(out.break_even_units, 0.0);
        assert_eq!(out.break_even_revenue, 0.0);
        assert_eq!(out.margin_of_safety_percentage, 100.0);
    }

    #[test]
    fn target_mos_above_full_is_infinite() {
        assert_eq!(sales_for_target_mos(75_000.0, 25.0), 100_000.0);
        assert!(sales_for_target_mos(75_000.0, 100.0).is_infinite());
        assert!(sales_for_target_mos(75_000.0, 120.0).is_infinite());
    }
}
