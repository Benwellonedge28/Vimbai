#![forbid(unsafe_code)]
//! Overhead absorption rates and over/under absorption, ported from
//! overhead-absorption-rate-service and over-under-absorption-service.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OarOutput {
    pub cost_centre_name: String,
    pub cost_centre_id: String,
    pub budgeted_overhead: f64,
    pub budgeted_base_units: f64,
    pub overhead_type: String,
    pub absorption_base: String,
    pub absorption_rate: f64,
    pub rate_per_unit: f64,
}

/// General OAR (overhead-absorption-rate-service `/calculate`):
/// `rate = budgeted_overhead / budgeted_base_units` (0 when base <= 0).
pub fn oar(
    cost_centre_name: &str,
    cost_centre_id: &str,
    budgeted_overhead: f64,
    budgeted_base_units: f64,
    overhead_type: &str,
    absorption_base: &str,
) -> OarOutput {
    let rate = if budgeted_base_units > 0.0 {
        budgeted_overhead / budgeted_base_units
    } else {
        0.0
    };
    OarOutput {
        cost_centre_name: cost_centre_name.to_string(),
        cost_centre_id: cost_centre_id.to_string(),
        budgeted_overhead,
        budgeted_base_units,
        overhead_type: overhead_type.to_string(),
        absorption_base: absorption_base.to_string(),
        absorption_rate: rate,
        rate_per_unit: rate,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateOutput {
    pub cost_centre_name: String,
    pub absorption_base: String,
    pub budgeted_overhead: f64,
    pub budgeted_base_units: f64,
    pub rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
}

/// Machine-hours rate: `overhead / machine_hours`.
pub fn machine_hours_rate(
    cost_centre_name: &str,
    budgeted_overhead: f64,
    budgeted_machine_hours: f64,
) -> RateOutput {
    let rate = if budgeted_machine_hours > 0.0 {
        budgeted_overhead / budgeted_machine_hours
    } else {
        0.0
    };
    RateOutput {
        cost_centre_name: cost_centre_name.to_string(),
        absorption_base: "machine_hours".to_string(),
        budgeted_overhead,
        budgeted_base_units: budgeted_machine_hours,
        rate,
        formula: Some(format!(
            "{} / {} = {}",
            budgeted_overhead, budgeted_machine_hours, rate
        )),
    }
}

/// Direct-labour-hours rate: `overhead / direct_labour_hours`.
pub fn labour_hours_rate(
    cost_centre_name: &str,
    budgeted_overhead: f64,
    budgeted_direct_labour_hours: f64,
) -> RateOutput {
    RateOutput {
        cost_centre_name: cost_centre_name.to_string(),
        absorption_base: "direct_labour_hours".to_string(),
        budgeted_overhead,
        budgeted_base_units: budgeted_direct_labour_hours,
        rate: if budgeted_direct_labour_hours > 0.0 {
            budgeted_overhead / budgeted_direct_labour_hours
        } else {
            0.0
        },
        formula: None,
    }
}

/// Direct-labour-cost percentage rate: `(overhead / labour_cost) * 100`.
pub fn labour_cost_rate(
    cost_centre_name: &str,
    budgeted_overhead: f64,
    budgeted_direct_labour_cost: f64,
) -> RateOutput {
    let rate = if budgeted_direct_labour_cost > 0.0 {
        budgeted_overhead / budgeted_direct_labour_cost * 100.0
    } else {
        0.0
    };
    RateOutput {
        cost_centre_name: cost_centre_name.to_string(),
        absorption_base: "direct_labour_cost".to_string(),
        budgeted_overhead,
        budgeted_base_units: budgeted_direct_labour_cost,
        rate,
        formula: Some(format!(
            "({} / {}) × 100 = {}%",
            budgeted_overhead, budgeted_direct_labour_cost, rate
        )),
    }
}

/// Material-cost percentage rate: `(overhead / material_cost) * 100`.
pub fn material_cost_rate(
    cost_centre_name: &str,
    budgeted_overhead: f64,
    budgeted_material_cost: f64,
) -> RateOutput {
    RateOutput {
        cost_centre_name: cost_centre_name.to_string(),
        absorption_base: "material_cost".to_string(),
        budgeted_overhead,
        budgeted_base_units: budgeted_material_cost,
        rate: if budgeted_material_cost > 0.0 {
            budgeted_overhead / budgeted_material_cost * 100.0
        } else {
            0.0
        },
        formula: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    OverheadSpending,
    Volume,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverUnderOutput {
    pub cost_centre_name: String,
    pub cost_centre_id: String,
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
    pub cause: Cause,
}

// Mirrors the FastAPI endpoint signature (8 explicit params).
#[allow(clippy::too_many_arguments)]
/// Full over/under absorption
/// (over-under-absorption-service `/calculate`).
///
/// `cause` mirrors the Python heuristic: spending wins when
/// `|spending_variance| > |variance - spending_variance|`, else volume.
pub fn over_under_absorption(
    cost_centre_name: &str,
    cost_centre_id: &str,
    period: &str,
    budgeted_overhead: f64,
    actual_overhead: f64,
    budgeted_base_units: f64,
    actual_base_units: f64,
    absorption_rate: f64,
) -> OverUnderOutput {
    let overhead_absorbed = actual_base_units * absorption_rate;
    let variance = overhead_absorbed - actual_overhead;
    let spending_variance = budgeted_overhead - actual_overhead;
    let cause = if spending_variance.abs() > (variance - spending_variance).abs() {
        Cause::OverheadSpending
    } else {
        Cause::Volume
    };
    OverUnderOutput {
        cost_centre_name: cost_centre_name.to_string(),
        cost_centre_id: cost_centre_id.to_string(),
        period: period.to_string(),
        budgeted_overhead,
        actual_overhead,
        budgeted_base_units,
        actual_base_units,
        absorption_rate,
        overhead_absorbed,
        over_absorption: if variance > 0.0 { variance } else { 0.0 },
        under_absorption: if variance < 0.0 { variance.abs() } else { 0.0 },
        total_variance: variance.abs(),
        cause,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleOverUnderOutput {
    pub overhead_absorbed: f64,
    pub actual_overhead: f64,
    pub variance: f64,
    pub status: &'static str,
    pub interpretation: String,
}

/// Simple over/under (over-under-absorption-service `/simple-calculation`).
pub fn simple_over_under(overhead_absorbed: f64, actual_overhead: f64) -> SimpleOverUnderOutput {
    let variance = overhead_absorbed - actual_overhead;
    let is_over = variance > 0.0;
    let status: &'static str = if is_over {
        "over_absorbed"
    } else {
        "under_absorbed"
    };
    SimpleOverUnderOutput {
        overhead_absorbed,
        actual_overhead,
        variance: variance.abs(),
        status,
        interpretation: format!(
            "Overhead was {} absorbed by {}",
            if is_over { "over" } else { "under" },
            variance.abs()
        ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingVarianceOutput {
    pub cost_centre_name: String,
    pub budgeted_overhead: f64,
    pub actual_overhead: f64,
    pub spending_variance: f64,
    pub is_favorable: bool,
    pub interpretation: String,
}

/// Spending variance (over-under-absorption-service `/spending-variance`).
pub fn spending_variance(
    cost_centre_name: &str,
    budgeted_overhead: f64,
    actual_overhead: f64,
) -> SpendingVarianceOutput {
    let variance = budgeted_overhead - actual_overhead;
    let favorable = variance > 0.0;
    SpendingVarianceOutput {
        cost_centre_name: cost_centre_name.to_string(),
        budgeted_overhead,
        actual_overhead,
        spending_variance: variance,
        is_favorable: favorable,
        interpretation: format!(
            "Spending variance is {}: {}",
            if favorable { "favorable" } else { "adverse" },
            variance.abs()
        ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeVarianceOutput {
    pub cost_centre_name: String,
    pub absorption_rate: f64,
    pub budgeted_base_units: f64,
    pub actual_base_units: f64,
    pub budgeted_absorbed: f64,
    pub actual_absorbed: f64,
    pub volume_variance: f64,
    pub is_favorable: bool,
}

/// Volume variance (over-under-absorption-service `/volume-variance`).
pub fn volume_variance(
    cost_centre_name: &str,
    absorption_rate: f64,
    budgeted_base_units: f64,
    actual_base_units: f64,
) -> VolumeVarianceOutput {
    let budgeted_absorbed = absorption_rate * budgeted_base_units;
    let actual_absorbed = absorption_rate * actual_base_units;
    let variance = actual_absorbed - budgeted_absorbed;
    VolumeVarianceOutput {
        cost_centre_name: cost_centre_name.to_string(),
        absorption_rate,
        budgeted_base_units,
        actual_base_units,
        budgeted_absorbed,
        actual_absorbed,
        volume_variance: variance,
        is_favorable: variance > 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oar_general() {
        let r = oar(
            "cutting",
            "cc1",
            50000.0,
            10000.0,
            "production",
            "machine_hours",
        );
        assert_eq!(r.absorption_rate, 5.0);
        assert_eq!(r.rate_per_unit, 5.0);
        let zero = oar("x", "cc", 500.0, 0.0, "p", "u");
        assert_eq!(zero.absorption_rate, 0.0);
    }

    #[test]
    fn base_specific_rates() {
        assert_eq!(machine_hours_rate("m", 48000.0, 8000.0).rate, 6.0);
        assert_eq!(labour_hours_rate("m", 48000.0, 12000.0).rate, 4.0);
        assert_eq!(labour_cost_rate("m", 30000.0, 150000.0).rate, 20.0);
        assert_eq!(material_cost_rate("m", 30000.0, 200000.0).rate, 15.0);
    }

    #[test]
    fn over_under_absorption_flow() {
        // absorbed = 9500 * 5 = 47500; actual = 48000 -> under 500
        let r = over_under_absorption("c", "cc", "2026-09", 50000.0, 48000.0, 10000.0, 9500.0, 5.0);
        assert_eq!(r.overhead_absorbed, 47500.0);
        assert_eq!(r.over_absorption, 0.0);
        assert_eq!(r.under_absorption, 500.0);
        assert_eq!(r.total_variance, 500.0);
        // spending = 50000-48000 = 2000; variance - spending = -500-2000 = -2500
        // |2000| < |-2500| -> volume
        assert_eq!(r.cause, Cause::Volume);
    }

    #[test]
    fn over_absorbed_case() {
        let r = over_under_absorption("c", "cc", "p", 40000.0, 41000.0, 10000.0, 10500.0, 4.0);
        // absorbed = 42000; variance = 1000 (over)
        assert_eq!(r.over_absorption, 1000.0);
        assert_eq!(r.under_absorption, 0.0);
        // spending = -1000; variance - spending = 2000; |-1000| < |2000| -> volume
        assert_eq!(r.cause, Cause::Volume);
    }

    #[test]
    fn simple_and_variances() {
        let s = simple_over_under(47500.0, 48000.0);
        assert_eq!(s.status, "under_absorbed");
        assert_eq!(s.variance, 500.0);
        let sv = spending_variance("cc", 50000.0, 48000.0);
        assert_eq!(sv.spending_variance, 2000.0);
        assert!(sv.is_favorable);
        let vv = volume_variance("cc", 5.0, 10000.0, 9500.0);
        assert_eq!(vv.volume_variance, -2500.0);
        assert!(!vv.is_favorable);
        assert_eq!(vv.actual_absorbed, 47500.0);
    }

    #[test]
    fn rates_are_unrounded_like_python() {
        let r = oar("c", "cc", 10000.0, 3.0, "p", "u");
        assert_eq!(r.absorption_rate, 10000.0 / 3.0);
    }
}
