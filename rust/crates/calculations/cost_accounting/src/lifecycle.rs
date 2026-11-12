#![forbid(unsafe_code)]
//! Lifecycle costing, ported from lifecycle-costing-service
//! (`/models`, `/compare`).

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecyclePhase {
    pub phase: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub duration_years: i64,
    #[serde(default)]
    pub annual_cost: f64,
    #[serde(default)]
    pub one_time_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleCostModel {
    pub asset_name: String,
    pub description: String,
    pub phases: Vec<LifecyclePhase>,
    pub total_lifecycle_cost: f64,
    pub annual_equivalent_cost: f64,
    pub discount_rate: f64,
}

/// Build a lifecycle cost model (lifecycle-costing-service `/models`):
/// `total = Σ(one_time + annual × duration)`;
/// `annual_equivalent = total / total_duration` (0 when no phases).
pub fn create_model(
    asset_name: &str,
    description: &str,
    discount_rate: f64,
    phases: Vec<LifecyclePhase>,
) -> LifecycleCostModel {
    let mut total = 0.0;
    let mut total_duration = 0.0;
    for p in &phases {
        total += p.one_time_cost + (p.annual_cost * p.duration_years as f64);
        total_duration += p.duration_years as f64;
    }
    let annual_equiv = if total_duration > 0.0 {
        total / total_duration
    } else {
        0.0
    };
    LifecycleCostModel {
        asset_name: asset_name.to_string(),
        description: description.to_string(),
        phases,
        total_lifecycle_cost: total,
        annual_equivalent_cost: py_round2(annual_equiv),
        discount_rate,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleComparisonEntry {
    pub model_index: usize,
    pub asset_name: String,
    pub total_lifecycle_cost: f64,
    pub annual_equivalent_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleComparison {
    pub comparison: Vec<LifecycleComparisonEntry>,
    /// Index of the lowest-total-cost model (None when empty).
    pub lowest_cost_model: Option<usize>,
}

/// Compare lifecycle models (lifecycle-costing-service `/compare`).
/// Ties pick the first model (Python `min` keeps the first minimum).
pub fn compare_models(models: &[LifecycleCostModel], indices: &[usize]) -> LifecycleComparison {
    let picked: Vec<(usize, &LifecycleCostModel)> = indices
        .iter()
        .copied()
        .filter_map(|i| models.get(i).map(|m| (i, m)))
        .collect();
    let comparison = picked
        .iter()
        .map(|(i, m)| LifecycleComparisonEntry {
            model_index: *i,
            asset_name: m.asset_name.clone(),
            total_lifecycle_cost: m.total_lifecycle_cost,
            annual_equivalent_cost: m.annual_equivalent_cost,
        })
        .collect();
    let lowest_cost_model = picked
        .iter()
        .min_by(|a, b| {
            a.1.total_lifecycle_cost
                .partial_cmp(&b.1.total_lifecycle_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| *i);
    LifecycleComparison {
        comparison,
        lowest_cost_model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phases() -> Vec<LifecyclePhase> {
        vec![
            LifecyclePhase {
                phase: "acquisition".into(),
                description: String::new(),
                duration_years: 1,
                annual_cost: 5000.0,
                one_time_cost: 100000.0,
            },
            LifecyclePhase {
                phase: "operation".into(),
                description: String::new(),
                duration_years: 10,
                annual_cost: 20000.0,
                one_time_cost: 0.0,
            },
        ]
    }

    #[test]
    fn model_totals() {
        let m = create_model("Machine A", "desc", 0.1, phases());
        // 105000 + 200000 = 305000; duration 11 -> 27727.27...
        assert_eq!(m.total_lifecycle_cost, 305000.0);
        assert_eq!(m.annual_equivalent_cost, py_round2(305000.0 / 11.0));
    }

    #[test]
    fn empty_model() {
        let m = create_model("X", "", 0.1, vec![]);
        assert_eq!(m.total_lifecycle_cost, 0.0);
        assert_eq!(m.annual_equivalent_cost, 0.0);
    }

    #[test]
    fn compare_picks_lowest() {
        let a = create_model("A", "", 0.1, phases());
        let mut cheaper = phases();
        cheaper[1].annual_cost = 10000.0;
        let b = create_model("B", "", 0.1, cheaper);
        let models = vec![a, b];
        let c = compare_models(&models, &[0, 1]);
        assert_eq!(c.comparison.len(), 2);
        assert_eq!(c.lowest_cost_model, Some(1));
        // out-of-range index ignored
        let partial = compare_models(&models, &[0, 5]);
        assert_eq!(partial.comparison.len(), 1);
        assert_eq!(partial.lowest_cost_model, Some(0));
    }
}
