//! labour-cost-variance-service: the rate and efficiency split of a labour
//! cost variance, department by department.
//!
//! PYTHON QUIRK: when either output figure is missing the standard hours are
//! used as they stand rather than scaled, so a department reporting hours but no
//! output has its whole hour gap counted. The
//! idle time total is always zero: nothing in the service ever measures it.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LabourData {
    pub department: String,
    pub standard_rate: f64,
    pub actual_rate: f64,
    pub standard_hours: f64,
    pub actual_hours: f64,
    #[serde(default)]
    pub standard_output: f64,
    #[serde(default)]
    pub actual_output: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabourVarianceRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub departments: Vec<LabourData>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DepartmentVariance {
    pub department: String,
    pub rate_variance: f64,
    pub efficiency_variance: f64,
    pub total_cost_variance: f64,
    pub standard_hours: f64,
    pub actual_hours: f64,
    pub standard_rate: f64,
    pub actual_rate: f64,
    /// A negative variance means the department spent less than standard.
    pub favorable_rate: bool,
    pub favorable_efficiency: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabourVarianceResult {
    pub id: String,
    pub company_id: String,
    pub period: String,
    pub total_rate_variance: f64,
    pub total_efficiency_variance: f64,
    pub total_cost_variance: f64,
    pub total_idle_time_variance: f64,
    pub departments: Vec<DepartmentVariance>,
}

pub fn analyse_labour_variance(
    req: &LabourVarianceRequest,
    report_id: &str,
) -> LabourVarianceResult {
    let mut total_rate = 0.0;
    let mut total_eff = 0.0;
    let mut total_cost = 0.0;
    let mut departments = Vec::with_capacity(req.departments.len());

    for dept in &req.departments {
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

        departments.push(DepartmentVariance {
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

    LabourVarianceResult {
        id: report_id.to_string(),
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        total_rate_variance: py_round2(total_rate),
        total_efficiency_variance: py_round2(total_eff),
        total_cost_variance: py_round2(total_cost),
        total_idle_time_variance: 0.0,
        departments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> LabourVarianceRequest {
        serde_json::from_value(json!({
            "company_id": "c1",
            "period": "2026-10",
            "departments": [
                {"department": "assembly", "standard_rate": 20, "actual_rate": 22,
                 "standard_hours": 1000, "actual_hours": 1100,
                 "standard_output": 500, "actual_output": 520},
                {"department": "finishing", "standard_rate": 18, "actual_rate": 17,
                 "standard_hours": 800, "actual_hours": 750,
                 "standard_output": 400, "actual_output": 410}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn the_rate_variance_uses_actual_hours() {
        let result = analyse_labour_variance(&request(), "id");
        // (22 - 20) * 1100
        assert_eq!(result.departments[0].rate_variance, 2200.0);
        // (17 - 18) * 750, favourable
        assert_eq!(result.departments[1].rate_variance, -750.0);
        assert!(result.departments[1].favorable_rate);
        assert!(!result.departments[0].favorable_rate);
    }

    #[test]
    fn the_efficiency_variance_uses_the_output_ratio() {
        let result = analyse_labour_variance(&request(), "id");
        // standard hours for 520 units = 1000/500*520 = 1040, so 1100 - 1040 = 60 h at 20
        assert_eq!(result.departments[0].efficiency_variance, 1200.0);
        // 800/400*410 = 820, so 750 - 820 = -70 h at 18
        assert_eq!(result.departments[1].efficiency_variance, -1260.0);
        assert!(result.departments[1].favorable_efficiency);
    }

    #[test]
    fn departments_roll_up_into_the_totals() {
        let result = analyse_labour_variance(&request(), "id");
        assert_eq!(result.total_rate_variance, 1450.0);
        assert_eq!(result.total_efficiency_variance, -60.0);
        assert_eq!(result.total_cost_variance, 1390.0);
        assert_eq!(result.total_idle_time_variance, 0.0);
        assert_eq!(result.id, "id");
    }

    #[test]
    fn without_output_the_standard_hours_stand_as_given() {
        let result = analyse_labour_variance(
            &LabourVarianceRequest {
                company_id: "c1".to_string(),
                departments: vec![LabourData {
                    department: "stores".to_string(),
                    standard_rate: 12.0,
                    actual_rate: 12.0,
                    standard_hours: 400.0,
                    actual_hours: 430.0,
                    ..Default::default()
                }],
                ..Default::default()
            },
            "id",
        );
        // no output means no scaling: the 400 standard hours are compared as
        // they stand, so all thirty extra hours land in the variance
        assert_eq!(result.departments[0].efficiency_variance, 360.0);
        assert_eq!(result.total_cost_variance, 360.0);
        assert!(!result.departments[0].favorable_efficiency);
    }

    #[test]
    fn an_empty_request_totals_zero() {
        let result = analyse_labour_variance(&LabourVarianceRequest::default(), "id");
        assert_eq!(result.total_cost_variance, 0.0);
        assert!(result.departments.is_empty());
    }
}
