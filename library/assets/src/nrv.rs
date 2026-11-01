//! net-realizable-value-service: NRV, scrap value, residual value,
//! inventory NRV and lower-of-cost-or-NRV comparison.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NrvOutput {
    pub selling_price: f64,
    pub completion_cost: f64,
    pub selling_cost: f64,
    pub net_realizable_value: f64,
}

pub fn nrv(selling_price: f64, completion_cost: f64, selling_cost: f64) -> NrvOutput {
    NrvOutput {
        selling_price,
        completion_cost,
        selling_cost,
        net_realizable_value: selling_price - completion_cost - selling_cost,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapValueOutput {
    pub net_book_value: f64,
    pub scrap_value: f64,
    pub gain_loss_on_disposal: f64,
}

/// Scrap value is proceeds minus disposal cost, but only when proceeds
/// were stated: Python returns 0 otherwise (even if disposal cost > 0).
pub fn scrap_value(
    original_cost: f64,
    accumulated_depreciation: f64,
    disposal_cost: f64,
    proceeds_from_sale: f64,
) -> ScrapValueOutput {
    let nbv = original_cost - accumulated_depreciation;
    let scrap = if proceeds_from_sale > 0.0 {
        proceeds_from_sale - disposal_cost
    } else {
        0.0
    };
    ScrapValueOutput {
        net_book_value: nbv,
        scrap_value: scrap,
        gain_loss_on_disposal: scrap - nbv,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidualValueOutput {
    pub total_depreciation: f64,
    pub residual_value: f64,
}

pub fn residual_value(
    original_cost: f64,
    depreciation_rate: f64,
    useful_life_years: i64,
) -> ResidualValueOutput {
    let total = original_cost * depreciation_rate * useful_life_years as f64;
    ResidualValueOutput {
        total_depreciation: total,
        residual_value: (original_cost - total).max(0.0),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostNrvComparison {
    pub cost: f64,
    pub nrv: f64,
    pub lower_of_cost_or_nrv: f64,
    pub write_down_required: bool,
    pub write_down_amount: f64,
}

/// compare-cost-nrv: valuation = min(cost, nrv); write-down only when
/// NRV is strictly below cost.
pub fn compare_cost_nrv(cost: f64, nrv_value: f64) -> CostNrvComparison {
    let lower = cost.min(nrv_value);
    let write_down = nrv_value < cost;
    CostNrvComparison {
        cost,
        nrv: nrv_value,
        lower_of_cost_or_nrv: lower,
        write_down_required: write_down,
        write_down_amount: if write_down { cost - nrv_value } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nrv_formula() {
        let r = nrv(100.0, 15.0, 5.0);
        assert_eq!(r.net_realizable_value, 80.0);
    }

    #[test]
    fn scrap_zero_without_proceeds() {
        let r = scrap_value(10000.0, 4000.0, 500.0, 0.0);
        assert_eq!(r.net_book_value, 6000.0);
        assert_eq!(r.scrap_value, 0.0);
        assert_eq!(r.gain_loss_on_disposal, -6000.0);
    }

    #[test]
    fn residual_floored_at_zero() {
        let r = residual_value(10000.0, 0.3, 5);
        assert_eq!(r.total_depreciation, 15000.0);
        assert_eq!(r.residual_value, 0.0);
    }

    #[test]
    fn cost_nrv_write_down() {
        let r = compare_cost_nrv(100.0, 85.0);
        assert_eq!(r.lower_of_cost_or_nrv, 85.0);
        assert!(r.write_down_required);
        assert_eq!(r.write_down_amount, 15.0);
        let equal = compare_cost_nrv(100.0, 100.0);
        assert!(!equal.write_down_required);
    }
}
