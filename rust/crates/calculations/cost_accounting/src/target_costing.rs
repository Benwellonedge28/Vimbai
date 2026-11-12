#![forbid(unsafe_code)]
//! Target costing, ported from target-costing-service. The Python
//! service caps feasibility at a 30% cost reduction.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetCostRequest {
    pub company_id: String,
    pub product_name: String,
    pub target_selling_price: f64,
    pub desired_profit_margin_pct: f64,
    pub current_cost: f64,
    /// Ordered (name, cost) pairs — Python dicts preserve insertion
    /// order, which the component analysis output inherits.
    #[serde(default)]
    pub component_costs: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentAnalysis {
    pub component: String,
    pub current_cost: f64,
    pub proportion_of_total: f64,
    pub suggested_reduction: f64,
    pub target_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetCostResult {
    pub company_id: String,
    pub product_name: String,
    pub target_selling_price: f64,
    pub desired_margin: f64,
    pub target_profit: f64,
    pub target_cost: f64,
    pub current_cost: f64,
    pub cost_reduction_needed: f64,
    pub cost_reduction_pct: f64,
    pub feasible: bool,
    pub component_analysis: Vec<ComponentAnalysis>,
}

/// Target costing (target-costing-service `/calculate`).
pub fn calculate_target_cost(req: &TargetCostRequest) -> TargetCostResult {
    let target_profit = req.target_selling_price * (req.desired_profit_margin_pct / 100.0);
    let target_cost = req.target_selling_price - target_profit;
    let cost_reduction = req.current_cost - target_cost;
    let cost_reduction_pct = if req.current_cost != 0.0 {
        cost_reduction / req.current_cost * 100.0
    } else {
        0.0
    };
    // 30% reduction is the max considered feasible
    let feasible = cost_reduction <= req.current_cost * 0.3;

    let total_component: f64 = req.component_costs.iter().map(|(_, c)| *c).sum();
    let component_analysis = req
        .component_costs
        .iter()
        .map(|(name, cost)| {
            let proportion = if total_component != 0.0 {
                cost / total_component
            } else {
                0.0
            };
            let reduction_needed = cost * (cost_reduction_pct / 100.0);
            ComponentAnalysis {
                component: name.clone(),
                current_cost: py_round2(*cost),
                proportion_of_total: py_round(proportion * 100.0, 1),
                suggested_reduction: py_round2(reduction_needed),
                target_cost: py_round2(cost - reduction_needed),
            }
        })
        .collect();

    TargetCostResult {
        company_id: req.company_id.clone(),
        product_name: req.product_name.clone(),
        target_selling_price: py_round2(req.target_selling_price),
        desired_margin: py_round2(req.desired_profit_margin_pct),
        target_profit: py_round2(target_profit),
        target_cost: py_round2(target_cost),
        current_cost: py_round2(req.current_cost),
        cost_reduction_needed: py_round2(cost_reduction),
        cost_reduction_pct: py_round(cost_reduction_pct, 2),
        feasible,
        component_analysis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> TargetCostRequest {
        let components = vec![
            ("materials".to_string(), 60.0),
            ("labour".to_string(), 40.0),
        ];
        TargetCostRequest {
            company_id: "c1".into(),
            product_name: "Widget".into(),
            target_selling_price: 150.0,
            desired_profit_margin_pct: 20.0,
            current_cost: 120.0,
            component_costs: components,
        }
    }

    #[test]
    fn target_cost_full_flow() {
        let r = calculate_target_cost(&req());
        assert_eq!(r.target_profit, 30.0);
        assert_eq!(r.target_cost, 120.0);
        assert_eq!(r.cost_reduction_needed, 0.0);
        assert!(r.feasible);
        let m = &r.component_analysis[0];
        assert_eq!(m.component, "materials");
        assert_eq!(m.proportion_of_total, 60.0);
        assert_eq!(m.suggested_reduction, 0.0);
    }

    #[test]
    fn infeasible_when_reduction_exceeds_30pct() {
        let mut rq = req();
        rq.current_cost = 200.0;
        rq.target_selling_price = 100.0;
        rq.desired_profit_margin_pct = 20.0;
        let r = calculate_target_cost(&rq);
        // reduction = 200 - 80 = 120 > 200*0.3 = 60
        assert!(!r.feasible);
        assert_eq!(r.cost_reduction_needed, 120.0);
        assert_eq!(r.cost_reduction_pct, 60.0);
        // suggested reductions split proportionally
        assert_eq!(
            r.component_analysis[0].suggested_reduction,
            py_round2(60.0 * 0.6)
        );
        assert_eq!(
            r.component_analysis[1].suggested_reduction,
            py_round2(40.0 * 0.6)
        );
    }

    #[test]
    fn zero_current_cost_guard() {
        let mut rq = req();
        rq.current_cost = 0.0;
        let r = calculate_target_cost(&rq);
        assert_eq!(r.cost_reduction_pct, 0.0);
        assert!(r.feasible); // 0 <= 0*0.3
    }
}
