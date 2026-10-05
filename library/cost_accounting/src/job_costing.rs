#![forbid(unsafe_code)]
//! Job costing rollups, ported from job-costing-service's `add_cost`
//! math (the Neo4j persistence and ownership checks stay in the
//! Python service until the seam is wired).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCosts {
    pub materials_cost: f64,
    pub labor_cost: f64,
    pub overhead_cost: f64,
    pub subcontractor_cost: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostType {
    Material,
    Labor,
    Overhead,
    Subcontractor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRollup {
    pub total_cost: f64,
    pub gross_profit: f64,
    pub gross_margin: f64,
}

/// Apply one cost entry to a job's running totals
/// (job-costing-service `crud.add_cost`).
/// `gross_margin = gross_profit / max(1, contract_value) * 100`.
pub fn add_cost(
    job: &JobCosts,
    cost_type: CostType,
    amount: f64,
    contract_value: f64,
) -> (JobCosts, JobRollup) {
    let mut updated = job.clone();
    match cost_type {
        CostType::Material => updated.materials_cost += amount,
        CostType::Labor => updated.labor_cost += amount,
        CostType::Overhead => updated.overhead_cost += amount,
        CostType::Subcontractor => updated.subcontractor_cost += amount,
    }
    let rollup = rollup(&updated, contract_value);
    (updated, rollup)
}

/// Recompute the job totals from its cost components
/// (`total_cost = materials + labor + overhead + subcontractor`).
pub fn rollup(job: &JobCosts, contract_value: f64) -> JobRollup {
    let total_cost =
        job.materials_cost + job.labor_cost + job.overhead_cost + job.subcontractor_cost;
    let gross_profit = contract_value - total_cost;
    let gross_margin = (gross_profit / contract_value.max(1.0)) * 100.0;
    JobRollup {
        total_cost,
        gross_profit,
        gross_margin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_costs_and_rollup() {
        let job = JobCosts {
            materials_cost: 0.0,
            labor_cost: 0.0,
            overhead_cost: 0.0,
            subcontractor_cost: 0.0,
        };
        let (j1, r1) = add_cost(&job, CostType::Material, 1000.0, 5000.0);
        assert_eq!(j1.materials_cost, 1000.0);
        assert_eq!(r1.total_cost, 1000.0);
        assert_eq!(r1.gross_profit, 4000.0);
        assert_eq!(r1.gross_margin, 80.0);
        let (j2, _r2) = add_cost(&j1, CostType::Labor, 500.0, 5000.0);
        let (j3, _r3) = add_cost(&j2, CostType::Overhead, 300.0, 5000.0);
        let (j4, r4) = add_cost(&j3, CostType::Subcontractor, 200.0, 5000.0);
        assert_eq!(j4.subcontractor_cost, 200.0);
        assert_eq!(r4.total_cost, 2000.0);
        assert_eq!(r4.gross_profit, 3000.0);
        assert_eq!(r4.gross_margin, 60.0);
    }

    #[test]
    fn margin_uses_max_one_contract_value() {
        let job = JobCosts {
            materials_cost: 50.0,
            labor_cost: 0.0,
            overhead_cost: 0.0,
            subcontractor_cost: 0.0,
        };
        let r = rollup(&job, 0.0);
        assert_eq!(r.gross_profit, -50.0);
        // contract_value clamps to 1 -> margin = -5000%
        assert_eq!(r.gross_margin, -5000.0);
    }
}
