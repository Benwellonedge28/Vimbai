#![forbid(unsafe_code)]
//! Cost centre registry logic, ported from cost-centre-service as pure
//! functions over caller-supplied slices (no global stores).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostCentre {
    pub centre_code: String,
    pub centre_name: String,
    pub centre_type: String,
    pub department_id: Option<String>,
    pub floor_area: f64,
    pub number_of_personnel: i64,
    pub machine_value: f64,
    pub is_service_centre: bool,
    pub parent_centre_id: Option<String>,
    pub number_of_requisitions: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostCentreCost {
    pub cost_centre_id: String,
    pub cost_code: String,
    pub cost_description: String,
    pub amount: f64,
    pub cost_type: String,
    pub period: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostCentreSummary {
    pub cost_centre_id: String,
    pub period: String,
    pub total_direct_costs: f64,
    pub total_indirect_costs: f64,
    pub total_overhead: f64,
    pub allocated_service_costs: f64,
    pub total_cost: f64,
}

/// Create a cost centre (cost-centre-service `/cost-centres/create`).
// Mirrors the FastAPI endpoint signature (9 explicit params).
#[allow(clippy::too_many_arguments)]
pub fn create_cost_centre(
    centre_code: &str,
    centre_name: &str,
    centre_type: &str,
    department_id: Option<&str>,
    floor_area: f64,
    number_of_personnel: i64,
    machine_value: f64,
    is_service_centre: bool,
    parent_centre_id: Option<&str>,
) -> CostCentre {
    CostCentre {
        centre_code: centre_code.to_string(),
        centre_name: centre_name.to_string(),
        centre_type: centre_type.to_string(),
        department_id: department_id.map(str::to_string),
        floor_area,
        number_of_personnel,
        machine_value,
        is_service_centre,
        parent_centre_id: parent_centre_id.map(str::to_string),
        number_of_requisitions: 0,
    }
}

/// Find a cost centre by id (client-side ids mirror the Python uuids).
pub fn find_centre<'a>(centres: &'a [CostCentre], centre_id: &str) -> Option<&'a CostCentre> {
    centres.iter().find(|c| c.centre_code == centre_id)
}

/// Generate a period summary for one cost centre
/// (cost-centre-service `/summary/generate`). Any cost type other than
/// direct/indirect falls into the `total_overhead` bucket.
pub fn generate_summary(
    costs: &[CostCentreCost],
    cost_centre_id: &str,
    period: &str,
    allocated_service_costs: f64,
) -> CostCentreSummary {
    let mut summary = CostCentreSummary {
        cost_centre_id: cost_centre_id.to_string(),
        period: period.to_string(),
        total_direct_costs: 0.0,
        total_indirect_costs: 0.0,
        total_overhead: 0.0,
        allocated_service_costs,
        total_cost: 0.0,
    };
    for cost in costs
        .iter()
        .filter(|c| c.cost_centre_id == cost_centre_id && c.period == period)
    {
        match cost.cost_type.as_str() {
            "direct" => summary.total_direct_costs += cost.amount,
            "indirect" => summary.total_indirect_costs += cost.amount,
            other => {
                if other != "direct" && other != "indirect" {
                    summary.total_overhead += cost.amount;
                }
            }
        }
    }
    summary.total_cost = summary.total_direct_costs
        + summary.total_indirect_costs
        + summary.total_overhead
        + allocated_service_costs;
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_direct_indirect_split() {
        let costs = vec![
            CostCentreCost {
                cost_centre_id: "cc1".into(),
                cost_code: "D1".into(),
                cost_description: "materials".into(),
                amount: 100.0,
                cost_type: "direct".into(),
                period: "2026-09".into(),
            },
            CostCentreCost {
                cost_centre_id: "cc1".into(),
                cost_code: "I1".into(),
                cost_description: "supervision".into(),
                amount: 50.0,
                cost_type: "indirect".into(),
                period: "2026-09".into(),
            },
            // unknown type falls into overhead bucket
            CostCentreCost {
                cost_centre_id: "cc1".into(),
                cost_code: "O1".into(),
                cost_description: "depreciation".into(),
                amount: 30.0,
                cost_type: "overhead".into(),
                period: "2026-09".into(),
            },
            // wrong period and wrong centre: excluded
            CostCentreCost {
                cost_centre_id: "cc1".into(),
                cost_code: "D2".into(),
                cost_description: "old".into(),
                amount: 999.0,
                cost_type: "direct".into(),
                period: "2026-08".into(),
            },
            CostCentreCost {
                cost_centre_id: "cc2".into(),
                cost_code: "D3".into(),
                cost_description: "other".into(),
                amount: 999.0,
                cost_type: "direct".into(),
                period: "2026-09".into(),
            },
        ];
        let s = generate_summary(&costs, "cc1", "2026-09", 25.0);
        assert_eq!(s.total_direct_costs, 100.0);
        assert_eq!(s.total_indirect_costs, 50.0);
        assert_eq!(s.total_overhead, 30.0);
        assert_eq!(s.total_cost, 205.0);
    }

    #[test]
    fn create_and_find() {
        let c = create_cost_centre(
            "cc1",
            "Assembly",
            "production",
            Some("dept1"),
            200.5,
            10,
            5000.0,
            false,
            None,
        );
        assert_eq!(c.number_of_requisitions, 0);
        let centres = vec![c];
        assert!(find_centre(&centres, "cc1").is_some());
        assert!(find_centre(&centres, "zz").is_none());
    }
}
