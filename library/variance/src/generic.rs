#![forbid(unsafe_code)]
//! Generic budgeted-vs-actual variance, ported from variance-service
//! `/calculate` and `/sales-variance`.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarianceKind {
    /// Budgeted - Actual (positive = under budget = favourable)
    Cost,
    /// Actual - Budgeted (positive = over revenue = favourable)
    Revenue,
}

/// Generic variance (variance-service `/calculate`).
pub fn cost_variance(budgeted: f64, actual: f64, kind: VarianceKind) -> f64 {
    py_round2(match kind {
        VarianceKind::Cost => budgeted - actual,
        VarianceKind::Revenue => actual - budgeted,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesVarianceOutput {
    pub standard_price: f64,
    pub actual_price: f64,
    pub standard_quantity: f64,
    pub actual_quantity: f64,
    pub standard_revenue: f64,
    pub actual_revenue: f64,
    pub total_sales_variance: f64,
    pub price_variance: f64,
    pub volume_variance: f64,
    pub price_variance_type: &'static str,
    pub volume_variance_type: &'static str,
}

/// Simple sales variance (variance-service `/sales-variance`).
pub fn revenue_variance(
    standard_price: f64,
    actual_price: f64,
    standard_quantity: f64,
    actual_quantity: f64,
) -> SalesVarianceOutput {
    let standard_revenue = standard_price * standard_quantity;
    let actual_revenue = actual_price * actual_quantity;
    let total_variance = actual_revenue - standard_revenue;
    let price_variance = (actual_price - standard_price) * actual_quantity;
    let volume_variance = (actual_quantity - standard_quantity) * standard_price;

    SalesVarianceOutput {
        standard_price,
        actual_price,
        standard_quantity,
        actual_quantity,
        standard_revenue,
        actual_revenue,
        total_sales_variance: py_round2(total_variance),
        price_variance: py_round2(price_variance),
        volume_variance: py_round2(volume_variance),
        price_variance_type: vimbai_common::favorable_adverse(price_variance),
        volume_variance_type: vimbai_common::favorable_adverse(volume_variance),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_variance_signs() {
        assert_eq!(cost_variance(100.0, 90.0, VarianceKind::Cost), 10.0);
        assert_eq!(cost_variance(100.0, 110.0, VarianceKind::Cost), -10.0);
        assert_eq!(cost_variance(100.0, 120.0, VarianceKind::Revenue), 20.0);
    }

    #[test]
    fn sales_variance_matches_python() {
        // SP=10 AP=12 SQ=50 AQ=60: std_rev=500, actual=720, total=220
        // price=(12-10)*60=120; volume=(60-50)*10=100
        let out = revenue_variance(10.0, 12.0, 50.0, 60.0);
        assert_eq!(out.total_sales_variance, 220.0);
        assert_eq!(out.price_variance, 120.0);
        assert_eq!(out.volume_variance, 100.0);
        assert_eq!(out.price_variance_type, "Favorable");
    }
}
