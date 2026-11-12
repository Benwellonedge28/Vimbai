#![forbid(unsafe_code)]
//! The cost-accounting-service calculation core: job costing and the
//! backward-compatible `/standards` variance view.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdditionalCost {
    pub description: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCostRequest {
    pub company_id: String,
    pub job_id: String,
    pub job_name: String,
    pub direct_materials: f64,
    pub direct_labour: f64,
    pub direct_labour_hours: f64,
    pub overhead_rate: f64,
    pub units_produced: f64,
    #[serde(default)]
    pub additional_costs: Vec<AdditionalCost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCostResult {
    pub company_id: String,
    pub job_id: String,
    pub job_name: String,
    pub direct_materials: f64,
    pub direct_labour: f64,
    pub applied_overhead: f64,
    pub additional_costs: f64,
    pub total_cost: f64,
    pub cost_per_unit: f64,
    pub cost_breakdown: BTreeMap<String, f64>,
}

/// Job cost with labour-hour applied overhead
/// (cost-accounting-service `/job-cost`).
/// Per-unit falls back to the FULL total when `units_produced == 0`
/// (Python parity quirk).
pub fn calculate_job_cost(req: &JobCostRequest) -> JobCostResult {
    let overhead = req.direct_labour_hours * req.overhead_rate;
    let additional: f64 = req.additional_costs.iter().map(|c| c.amount).sum();
    let total = req.direct_materials + req.direct_labour + overhead + additional;
    let per_unit = if req.units_produced != 0.0 {
        total / req.units_produced
    } else {
        total
    };
    let mut cost_breakdown = BTreeMap::new();
    cost_breakdown.insert("direct_materials".into(), py_round2(req.direct_materials));
    cost_breakdown.insert("direct_labour".into(), py_round2(req.direct_labour));
    cost_breakdown.insert("applied_overhead".into(), py_round2(overhead));
    cost_breakdown.insert("additional_costs".into(), py_round2(additional));
    cost_breakdown.insert("total".into(), py_round2(total));
    JobCostResult {
        company_id: req.company_id.clone(),
        job_id: req.job_id.clone(),
        job_name: req.job_name.clone(),
        direct_materials: py_round2(req.direct_materials),
        direct_labour: py_round2(req.direct_labour),
        applied_overhead: py_round2(overhead),
        additional_costs: py_round2(additional),
        total_cost: py_round2(total),
        cost_per_unit: py_round2(per_unit),
        cost_breakdown,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardCostReq {
    pub company_id: String,
    pub product_name: String,
    pub direct_materials_std: f64,
    pub direct_labor_std: f64,
    pub overhead_std: f64,
    pub units_produced: i64,
    pub actual_materials: f64,
    pub actual_labor: f64,
    pub actual_overhead: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardCostResult {
    pub standard_cost_per_unit: f64,
    pub actual_total: f64,
    pub actual_cost_per_unit: f64,
    pub material_variance: f64,
    pub labor_variance: f64,
    pub overhead_variance: f64,
    pub total_variance: f64,
}

/// Standard vs actual costing view
/// (cost-accounting-service `/standards`).
pub fn standard_costing(req: &StandardCostReq) -> StandardCostResult {
    let std_cost_per_unit = req.direct_materials_std + req.direct_labor_std + req.overhead_std;
    let actual_total = req.actual_materials + req.actual_labor + req.actual_overhead;
    let units = req.units_produced as f64;
    let actual_cost_per_unit = if units != 0.0 {
        actual_total / units
    } else {
        0.0
    };
    let mat_var = req.actual_materials - (req.direct_materials_std * units);
    let lab_var = req.actual_labor - (req.direct_labor_std * units);
    let ovh_var = req.actual_overhead - (req.overhead_std * units);
    StandardCostResult {
        standard_cost_per_unit: std_cost_per_unit,
        actual_total,
        actual_cost_per_unit,
        material_variance: mat_var,
        labor_variance: lab_var,
        overhead_variance: ovh_var,
        total_variance: mat_var + lab_var + ovh_var,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_cost_flow() {
        let req = JobCostRequest {
            company_id: "c1".into(),
            job_id: "j1".into(),
            job_name: "Renovation".into(),
            direct_materials: 10000.0,
            direct_labour: 8000.0,
            direct_labour_hours: 400.0,
            overhead_rate: 15.0,
            units_produced: 100.0,
            additional_costs: vec![
                AdditionalCost {
                    description: "permits".into(),
                    amount: 500.0,
                },
                AdditionalCost {
                    description: "haulage".into(),
                    amount: 300.0,
                },
            ],
        };
        let r = calculate_job_cost(&req);
        assert_eq!(r.applied_overhead, 6000.0);
        assert_eq!(r.additional_costs, 800.0);
        assert_eq!(r.total_cost, 24800.0);
        assert_eq!(r.cost_per_unit, 248.0);
        assert_eq!(r.cost_breakdown["applied_overhead"], 6000.0);
    }

    #[test]
    fn job_cost_zero_units_falls_back_to_total() {
        let req = JobCostRequest {
            company_id: "c1".into(),
            job_id: "j1".into(),
            job_name: "X".into(),
            direct_materials: 1000.0,
            direct_labour: 500.0,
            direct_labour_hours: 10.0,
            overhead_rate: 5.0,
            units_produced: 0.0,
            additional_costs: vec![],
        };
        let r = calculate_job_cost(&req);
        // Python: per_unit = total when units == 0
        assert_eq!(r.cost_per_unit, 1550.0);
    }

    #[test]
    fn standards_variances() {
        let req = StandardCostReq {
            company_id: "c1".into(),
            product_name: "W".into(),
            direct_materials_std: 10.0,
            direct_labor_std: 5.0,
            overhead_std: 3.0,
            units_produced: 1000,
            actual_materials: 10500.0,
            actual_labor: 4900.0,
            actual_overhead: 3200.0,
        };
        let r = standard_costing(&req);
        assert_eq!(r.standard_cost_per_unit, 18.0);
        assert_eq!(r.actual_total, 18600.0);
        assert_eq!(r.actual_cost_per_unit, 18.6);
        assert_eq!(r.material_variance, 500.0);
        assert_eq!(r.labor_variance, -100.0);
        assert_eq!(r.overhead_variance, 200.0);
        assert_eq!(r.total_variance, 600.0);
    }

    #[test]
    fn standards_zero_units() {
        let req = StandardCostReq {
            company_id: "c1".into(),
            product_name: "W".into(),
            direct_materials_std: 1.0,
            direct_labor_std: 1.0,
            overhead_std: 1.0,
            units_produced: 0,
            actual_materials: 5.0,
            actual_labor: 5.0,
            actual_overhead: 5.0,
        };
        let r = standard_costing(&req);
        assert_eq!(r.actual_cost_per_unit, 0.0);
        assert_eq!(r.total_variance, 15.0);
    }
}
