#![forbid(unsafe_code)]
//! Job costing rollups, ported from job-costing-service's `add_cost`
//! math (the Neo4j persistence and ownership checks stay in the
//! Python service until the seam is wired).

use serde::{Deserialize, Serialize};
use vimbai_common::Money;

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
///
/// The arithmetic is `vimbai_service_costing::job_costing::settle`, the same function the
/// record-backed endpoint uses. This crate used to carry its own copy in `f64`, with a
/// `max(1, contract_value)` guard the exact version does not have, so the seam and the API could
/// answer differently for the same job. `f64` survives here only because the seam's request and
/// response shapes are `f64`; the sums themselves are exact.
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
    let total = components(job)
        .iter()
        .fold(Money::zero(), |running, cost| running.plus(cost));
    let contract = money(contract_value);
    let settled = vimbai_service_costing::job_costing::settle(&total, &contract);
    JobRollup {
        total_cost: settled.total.to_f64_lossy(),
        gross_profit: settled.profit.to_f64_lossy(),
        gross_margin: settled.margin,
    }
}

fn components(job: &JobCosts) -> [Money; 4] {
    [
        money(job.materials_cost),
        money(job.labor_cost),
        money(job.overhead_cost),
        money(job.subcontractor_cost),
    ]
}

fn money(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
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

    /// The Python service divided by `max(1, contract_value)`, which turned a contract of zero
    /// into a margin of -5000% and a contract of 0.50 into a margin computed against 1. Both
    /// figures stared back from a report as if they meant something. A contract of zero has no
    /// margin to report, so it reports none, and any real contract divides by itself.
    #[test]
    fn a_zero_contract_reports_no_margin() {
        let job = JobCosts {
            materials_cost: 50.0,
            labor_cost: 0.0,
            overhead_cost: 0.0,
            subcontractor_cost: 0.0,
        };
        let r = rollup(&job, 0.0);
        assert_eq!(r.gross_profit, -50.0);
        assert_eq!(r.gross_margin, 0.0);
    }

    #[test]
    fn a_contract_below_one_divides_by_itself() {
        let job = JobCosts {
            materials_cost: 0.25,
            labor_cost: 0.0,
            overhead_cost: 0.0,
            subcontractor_cost: 0.0,
        };
        let r = rollup(&job, 0.5);
        // 0.25 profit on a 0.50 contract is 50%, not 25%.
        assert_eq!(r.gross_profit, 0.25);
        assert_eq!(r.gross_margin, 50.0);
    }
}
