#![forbid(unsafe_code)]
//! Over/under absorption and overhead variance analysis, ported from
//! over-under-absorption-service.
//!
//! Quirks kept on purpose: the cause of a net variance is attributed to
//! overhead spending only when the spending variance is strictly larger in
//! absolute size than the remainder, so a tie reports "volume"; a zero net
//! variance counts as under-absorbed because the test is `> 0`; and the
//! spending variance is budgeted minus actual, so a favourable figure is
//! the positive one.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverUnderAbsorptionOutput {
    pub cost_centre_name: String,
    pub period: String,
    pub budgeted_overhead: f64,
    pub actual_overhead: f64,
    pub budgeted_base_units: f64,
    pub actual_base_units: f64,
    pub absorption_rate: f64,
    pub overhead_absorbed: f64,
    pub over_absorption: f64,
    pub under_absorption: f64,
    pub total_variance: f64,
    pub cause: String,
}

#[allow(clippy::too_many_arguments)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimpleOverUnderOutput {
    pub overhead_absorbed: f64,
    pub actual_overhead: f64,
    pub variance: f64,
    pub status: String,
    pub interpretation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpendingVarianceOutput {
    pub budgeted_overhead: f64,
    pub actual_overhead: f64,
    pub spending_variance: f64,
    pub is_favorable: bool,
    pub interpretation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VolumeVarianceOutput {
    pub absorption_rate: f64,
    pub budgeted_base_units: f64,
    pub actual_base_units: f64,
    pub budgeted_absorbed: f64,
    pub actual_absorbed: f64,
    pub volume_variance: f64,
    pub is_favorable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacityVarianceOutput {
    pub absorption_rate: f64,
    pub budgeted_capacity: f64,
    pub actual_capacity: f64,
    pub idle_capacity: f64,
    pub capacity_variance: f64,
    pub is_favorable: bool,
}

/// `/capacity-variance`
pub fn capacity_variance(
    absorption_rate: f64,
    budgeted_capacity: f64,
    actual_capacity: f64,
) -> CapacityVarianceOutput {
    let variance = absorption_rate * (actual_capacity - budgeted_capacity);
    CapacityVarianceOutput {
        absorption_rate,
        budgeted_capacity,
        actual_capacity,
        idle_capacity: budgeted_capacity - actual_capacity,
        capacity_variance: variance,
        is_favorable: variance > 0.0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComprehensiveVarianceOutput {
    pub cost_centre_name: String,
    pub period: String,
    pub budgeted_overhead: f64,
    pub actual_overhead: f64,
    pub overhead_absorbed: f64,
    pub budgeted_base_units: f64,
    pub actual_base_units: f64,
    pub absorption_rate: f64,
    pub spending_variance: f64,
    pub spending_favorable: bool,
    pub volume_variance: f64,
    pub volume_favorable: bool,
    pub total_variance: f64,
    pub status: String,
}

#[allow(clippy::too_many_arguments)]
/// `/comprehensive-analysis`
pub fn comprehensive_variance(
    cost_centre_name: &str,
    period: &str,
    budgeted_overhead: f64,
    actual_overhead: f64,
    budgeted_base_units: f64,
    actual_base_units: f64,
    absorption_rate: f64,
) -> ComprehensiveVarianceOutput {
    let overhead_absorbed = absorption_rate * actual_base_units;
    let spending = budgeted_overhead - actual_overhead;
    let budgeted_absorbed = absorption_rate * budgeted_base_units;
    let volume = overhead_absorbed - budgeted_absorbed;
    let total = overhead_absorbed - actual_overhead;
    let is_over = total > 0.0;
    ComprehensiveVarianceOutput {
        cost_centre_name: cost_centre_name.to_string(),
        period: period.to_string(),
        budgeted_overhead,
        actual_overhead,
        overhead_absorbed,
        budgeted_base_units,
        actual_base_units,
        absorption_rate,
        spending_variance: spending,
        spending_favorable: spending > 0.0,
        volume_variance: volume,
        volume_favorable: volume > 0.0,
        total_variance: total,
        status: if is_over {
            "over_absorbed".to_string()
        } else {
            "under_absorbed".to_string()
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverUnderTreatmentOutput {
    pub over_absorption: f64,
    pub under_absorption: f64,
    pub net_variance: f64,
    pub treatment: String,
    pub recommendation: String,
}

/// `/adjust-over-under`
pub fn adjust_over_under(
    over_absorption: f64,
    under_absorption: f64,
    write_off_to_costing: bool,
) -> OverUnderTreatmentOutput {
    OverUnderTreatmentOutput {
        over_absorption,
        under_absorption,
        net_variance: over_absorption - under_absorption,
        treatment: if write_off_to_costing {
            "write_off_to_costing_profit_loss".to_string()
        } else {
            "carry_forward_in_costing_records".to_string()
        },
        recommendation: "Over-absorption: reduce cost of production; Under-absorption: write off to P&L or carry forward"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comprehensive_analysis_combines_the_pieces() {
        let out = comprehensive_variance(
            "Assembly", "2026-01", 100_000.0, 90_000.0, 10_000.0, 11_000.0, 10.0,
        );
        assert_eq!(out.overhead_absorbed, 110_000.0);
        assert_eq!(out.spending_variance, 10_000.0);
        assert_eq!(out.volume_variance, 10_000.0);
        assert_eq!(out.total_variance, 20_000.0);
        assert_eq!(out.status, "over_absorbed");
    }

    #[test]
    fn treatment_follows_the_flag() {
        let carried = adjust_over_under(20_000.0, 5_000.0, false);
        assert_eq!(carried.net_variance, 15_000.0);
        assert_eq!(carried.treatment, "carry_forward_in_costing_records");
        let written = adjust_over_under(20_000.0, 5_000.0, true);
        assert_eq!(written.treatment, "write_off_to_costing_profit_loss");
    }
}
