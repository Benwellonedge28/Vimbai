#![forbid(unsafe_code)]
//! Labour variances, ported from labour-rate-variance-service,
//! labour-efficiency-variance-service, labour-cost-variance-service and
//! variance-service `/labour-variance`.
//!
//! Formulas (parity with the Python services):
//!
//! 1. Rate Variance = (Standard Rate - Actual Rate) x Actual Hours
//! 2. Efficiency Variance = (Standard Hours - Actual Hours) x Standard Rate
//!    (when standard_hours is zero, expected hours = actual_output x
//!    standard_hours_per_unit is used instead)
//! 3. Idle Time Variance = -(Idle Hours x Standard Rate)
//! 4. Department cost-variance analysis (labour-cost service) uses the
//!    *cost* sign convention (rate = (AR - SR) x AH) and per-department
//!    standard hours flexed to actual output.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabourVarianceInput {
    #[serde(default)]
    pub company_id: String,
    #[serde(default)]
    pub department: String,
    #[serde(default)]
    pub period: String,
    pub standard_rate: f64,
    pub actual_rate: f64,
    pub standard_hours: f64,
    pub actual_hours: f64,
    #[serde(default)]
    pub idle_hours: f64,
    #[serde(default)]
    pub standard_hours_per_unit: f64,
    #[serde(default)]
    pub actual_output: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabourVarianceOutput {
    pub company_id: String,
    pub department: String,
    pub period: String,
    pub rate_variance: f64,
    pub efficiency_variance: f64,
    pub idle_time_variance: f64,
    pub total_variance: f64,
    pub favourable: bool,
    #[serde(default)]
    pub analysis: std::collections::BTreeMap<String, String>,
}

/// Rate variance (labour-rate service): `(SR - AR) x AH`.
/// A positive result means paying below standard (favourable).
pub fn labour_rate_variance(standard_rate: f64, actual_rate: f64, actual_hours: f64) -> f64 {
    py_round2((standard_rate - actual_rate) * actual_hours)
}

/// Full single-department labour variance (labour-efficiency service).
pub fn labour_variance(input: &LabourVarianceInput) -> LabourVarianceOutput {
    let expected_hours = input.actual_output * input.standard_hours_per_unit;

    let rate_var = (input.standard_rate - input.actual_rate) * input.actual_hours;
    let eff_var = if input.standard_hours != 0.0 {
        (input.standard_hours - input.actual_hours) * input.standard_rate
    } else {
        (expected_hours - input.actual_hours) * input.standard_rate
    };
    let idle_var = -(input.idle_hours * input.standard_rate);
    let total = rate_var + eff_var + idle_var;

    let mut analysis = std::collections::BTreeMap::new();
    if rate_var < 0.0 {
        analysis.insert(
            "rate".to_string(),
            format!(
                "Unfavourable: paying {} vs standard {} - negotiate or review pay scales",
                input.actual_rate, input.standard_rate
            ),
        );
    } else {
        analysis.insert(
            "rate".to_string(),
            "Favourable: paying below standard rate".to_string(),
        );
    }
    if eff_var < 0.0 {
        analysis.insert(
            "efficiency".to_string(),
            format!(
                "Unfavourable: {}h actual vs {}h expected - training or process improvement needed",
                input.actual_hours, expected_hours
            ),
        );
    } else {
        analysis.insert(
            "efficiency".to_string(),
            "Favourable: fewer hours than standard".to_string(),
        );
    }
    if idle_var < 0.0 {
        analysis.insert(
            "idle_time".to_string(),
            format!(
                "Unfavourable: {}h idle time costing {}",
                input.idle_hours,
                py_round2(idle_var.abs())
            ),
        );
    }

    LabourVarianceOutput {
        company_id: input.company_id.clone(),
        department: input.department.clone(),
        period: input.period.clone(),
        rate_variance: py_round2(rate_var),
        efficiency_variance: py_round2(eff_var),
        idle_time_variance: py_round2(idle_var),
        total_variance: py_round2(total),
        favourable: total >= 0.0,
        analysis,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentInput {
    pub department: String,
    pub standard_rate: f64,
    pub actual_rate: f64,
    pub standard_hours: f64,
    pub actual_hours: f64,
    pub standard_output: f64,
    pub actual_output: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentResult {
    pub department: String,
    pub rate_variance: f64,
    pub efficiency_variance: f64,
    pub total_cost_variance: f64,
    pub standard_hours: f64,
    pub actual_hours: f64,
    pub standard_rate: f64,
    pub actual_rate: f64,
    pub favorable_rate: bool,
    pub favorable_efficiency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentAnalysisOutput {
    pub company_id: String,
    pub period: String,
    pub total_rate_variance: f64,
    pub total_efficiency_variance: f64,
    pub total_cost_variance: f64,
    #[serde(default)]
    pub total_idle_time_variance: f64,
    pub departments: Vec<DepartmentResult>,
}

/// Multi-department analysis, ported from labour-cost-variance-service.
/// Cost convention: rate = (AR - SR) x AH; efficiency flexed to actual output.
pub fn department_labour_analysis(
    company_id: &str,
    period: &str,
    departments: &[DepartmentInput],
) -> DepartmentAnalysisOutput {
    let mut total_rate = 0.0;
    let mut total_eff = 0.0;
    let mut total_cost = 0.0;
    let mut results = Vec::with_capacity(departments.len());

    for dept in departments {
        let rate_variance = (dept.actual_rate - dept.standard_rate) * dept.actual_hours;

        let std_hours_for_actual = if dept.actual_output > 0.0 && dept.standard_output > 0.0 {
            (dept.standard_hours / dept.standard_output) * dept.actual_output
        } else {
            dept.standard_hours
        };

        let efficiency_variance = (dept.actual_hours - std_hours_for_actual) * dept.standard_rate;
        let cost_variance = rate_variance + efficiency_variance;

        total_rate += rate_variance;
        total_eff += efficiency_variance;
        total_cost += cost_variance;

        results.push(DepartmentResult {
            department: dept.department.clone(),
            rate_variance: py_round2(rate_variance),
            efficiency_variance: py_round2(efficiency_variance),
            total_cost_variance: py_round2(cost_variance),
            standard_hours: dept.standard_hours,
            actual_hours: dept.actual_hours,
            standard_rate: dept.standard_rate,
            actual_rate: dept.actual_rate,
            favorable_rate: rate_variance < 0.0,
            favorable_efficiency: efficiency_variance < 0.0,
        });
    }

    DepartmentAnalysisOutput {
        company_id: company_id.to_string(),
        period: period.to_string(),
        total_rate_variance: py_round2(total_rate),
        total_efficiency_variance: py_round2(total_eff),
        total_cost_variance: py_round2(total_cost),
        total_idle_time_variance: 0.0,
        departments: results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_variance_matches_python_service() {
        // python: /calculate-rate-variance?standard_rate=15&actual_rate=16&actual_hours=1000
        // variance = (15 - 16) * 1000 = -1000.0
        assert_eq!(labour_rate_variance(15.0, 16.0, 1000.0), -1000.0);
    }

    #[test]
    fn efficiency_variance_uses_expected_hours_when_standard_is_zero() {
        let input = LabourVarianceInput {
            company_id: "c1".into(),
            department: "assembly".into(),
            period: "2026-09".into(),
            standard_rate: 20.0,
            actual_rate: 20.0,
            standard_hours: 0.0,
            actual_hours: 950.0,
            idle_hours: 10.0,
            standard_hours_per_unit: 2.0,
            actual_output: 500.0,
        };
        let out = labour_variance(&input);
        // expected_hours = 500 * 2 = 1000; eff = (1000 - 950) * 20 = 1000
        // idle = -(10 * 20) = -200; total = 0 + 1000 - 200 = 800
        assert_eq!(out.efficiency_variance, 1000.0);
        assert_eq!(out.idle_time_variance, -200.0);
        assert_eq!(out.total_variance, 800.0);
        assert!(out.favourable);
    }

    #[test]
    fn department_analysis_flexes_standard_hours() {
        let depts = vec![DepartmentInput {
            department: "cutting".into(),
            standard_rate: 12.0,
            actual_rate: 13.0,
            standard_hours: 400.0,
            actual_hours: 390.0,
            standard_output: 200.0,
            actual_output: 180.0,
        }];
        let out = department_labour_analysis("c1", "2026-09", &depts);
        // rate = (13-12)*390 = 390; std_hours_for_actual = (400/200)*180 = 360
        // eff = (390-360)*12 = 360; cost = 750
        assert_eq!(out.departments[0].rate_variance, 390.0);
        assert_eq!(out.departments[0].efficiency_variance, 360.0);
        assert_eq!(out.departments[0].total_cost_variance, 750.0);
        assert!(!out.departments[0].favorable_rate);
        assert!(!out.departments[0].favorable_efficiency);
        assert_eq!(out.total_cost_variance, 750.0);
    }
}
