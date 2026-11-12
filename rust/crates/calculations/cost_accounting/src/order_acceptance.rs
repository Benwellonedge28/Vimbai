#![forbid(unsafe_code)]
//! Order acceptance analysis, ported from order-acceptance-service
//! (`/analyze` and `/one-time-order`). Storage-free.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAcceptanceDecision {
    pub order_id: String,
    pub product_id: String,
    pub product_name: String,
    pub customer_name: String,
    pub normal_selling_price: f64,
    pub offered_price: f64,
    pub variable_cost_per_unit: f64,
    pub units_requested: f64,
    pub total_fixed_cost_incremental: f64,
    pub price_below_normal: f64,
    pub price_below_normal_percentage: f64,
    pub contribution_per_unit_at_normal: f64,
    pub contribution_per_unit_at_offered: f64,
    pub contribution_from_order: f64,
    pub total_relevant_cost: f64,
    pub minimum_acceptable_price: f64,
    pub opportunity_cost: f64,
    pub net_contribution: f64,
    pub full_cost_per_unit: Option<f64>,
    pub recommendation: &'static str,
    pub conditions: Vec<String>,
}

/// Full order-acceptance analysis
/// (order-acceptance-service `/analyze`).
// Mirrors the FastAPI endpoint signature (13 explicit params).
#[allow(clippy::too_many_arguments)]
pub fn analyze_order_acceptance(
    order_id: &str,
    product_id: &str,
    product_name: &str,
    customer_name: &str,
    normal_selling_price: f64,
    offered_price: f64,
    variable_cost_per_unit: f64,
    units_requested: f64,
    full_cost_per_unit: Option<f64>,
    incremental_fixed_costs: f64,
    displaced_sales_units: f64,
    displaced_contribution_per_unit: f64,
) -> OrderAcceptanceDecision {
    let price_below_normal = normal_selling_price - offered_price;
    let price_below_normal_percentage = if normal_selling_price > 0.0 {
        price_below_normal / normal_selling_price * 100.0
    } else {
        0.0
    };
    let contribution_per_unit_at_normal = normal_selling_price - variable_cost_per_unit;
    let contribution_per_unit_at_offered = offered_price - variable_cost_per_unit;
    let contribution_from_order = contribution_per_unit_at_offered * units_requested;
    let total_relevant_cost = variable_cost_per_unit * units_requested + incremental_fixed_costs;
    let minimum_acceptable_price = if units_requested > 0.0 {
        variable_cost_per_unit + (incremental_fixed_costs / units_requested)
    } else {
        0.0
    };
    let opportunity_cost = displaced_sales_units * displaced_contribution_per_unit;
    let net_contribution = contribution_from_order - opportunity_cost;

    let (recommendation, conditions): (&'static str, Vec<String>) = if opportunity_cost == 0.0 {
        if contribution_per_unit_at_offered > 0.0 {
            (
                "ACCEPT",
                vec!["Positive contribution generated".to_string()],
            )
        } else {
            (
                "REJECT",
                vec!["Negative contribution - would lose money".to_string()],
            )
        }
    } else if net_contribution > 0.0 {
        (
            "ACCEPT_WITH_CONDITIONS",
            vec![
                format!("Net contribution: {net_contribution:?}"),
                "Consider impact on existing customers".to_string(),
            ],
        )
    } else if net_contribution == 0.0 {
        (
            "INDIFFERENT",
            vec!["Consider strategic value of customer relationship".to_string()],
        )
    } else {
        (
            "REJECT",
            vec![
                format!("Net contribution negative: {net_contribution:?}"),
                "Would reduce overall profitability".to_string(),
            ],
        )
    };

    OrderAcceptanceDecision {
        order_id: order_id.to_string(),
        product_id: product_id.to_string(),
        product_name: product_name.to_string(),
        customer_name: customer_name.to_string(),
        normal_selling_price,
        offered_price,
        variable_cost_per_unit,
        units_requested,
        total_fixed_cost_incremental: incremental_fixed_costs,
        price_below_normal,
        price_below_normal_percentage,
        contribution_per_unit_at_normal,
        contribution_per_unit_at_offered,
        contribution_from_order,
        total_relevant_cost,
        minimum_acceptable_price,
        opportunity_cost,
        net_contribution,
        full_cost_per_unit,
        recommendation,
        conditions,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneTimeOrderAnalysis {
    pub product_name: String,
    pub offered_price: f64,
    pub variable_cost_per_unit: f64,
    pub units: f64,
    pub contribution_per_unit: f64,
    pub total_contribution: f64,
    pub minimum_acceptable_price: f64,
    pub has_spare_capacity: bool,
    pub recommendation: &'static str,
    pub reason: String,
}

/// Quick one-time special-order analysis
/// (order-acceptance-service `/one-time-order`). Reason strings match
/// the Python f-strings exactly.
pub fn analyze_one_time_order(
    product_name: &str,
    offered_price: f64,
    variable_cost_per_unit: f64,
    units: f64,
    has_spare_capacity: bool,
) -> OneTimeOrderAnalysis {
    let contribution_per_unit = offered_price - variable_cost_per_unit;
    let total_contribution = contribution_per_unit * units;
    let (recommendation, reason): (&'static str, String) = if has_spare_capacity {
        if contribution_per_unit > 0.0 {
            (
                "ACCEPT",
                format!("Spare capacity available. Generates {total_contribution:?} contribution."),
            )
        } else {
            ("REJECT", "Price below variable cost.".to_string())
        }
    } else if contribution_per_unit > 0.0 {
        (
            "CONSIDER",
            "No spare capacity - would need to reduce regular production".to_string(),
        )
    } else {
        ("REJECT", "Price below variable cost".to_string())
    };
    OneTimeOrderAnalysis {
        product_name: product_name.to_string(),
        offered_price,
        variable_cost_per_unit,
        units,
        contribution_per_unit,
        total_contribution,
        minimum_acceptable_price: variable_cost_per_unit,
        has_spare_capacity,
        recommendation,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_positive_contribution() {
        let d = analyze_order_acceptance(
            "o1", "p1", "Widget", "Acme", 100.0, 85.0, 60.0, 500.0, None, 0.0, 0.0, 0.0,
        );
        assert_eq!(d.recommendation, "ACCEPT");
        assert_eq!(d.price_below_normal, 15.0);
        assert_eq!(d.price_below_normal_percentage, 15.0);
        assert_eq!(d.contribution_per_unit_at_offered, 25.0);
        assert_eq!(d.contribution_from_order, 12500.0);
        assert_eq!(d.opportunity_cost, 0.0);
        assert_eq!(d.net_contribution, 12500.0);
    }

    #[test]
    fn displaced_sales_paths() {
        let d = analyze_order_acceptance(
            "o2", "p1", "Widget", "Acme", 100.0, 85.0, 60.0, 500.0, None, 0.0, 100.0, 30.0,
        );
        // net = 12500 - 3000 = 9500 > 0
        assert_eq!(d.recommendation, "ACCEPT_WITH_CONDITIONS");
        assert_eq!(d.net_contribution, 9500.0);
        assert!(d.conditions[0].contains("Net contribution: 9500"));

        let zero = analyze_order_acceptance(
            "o3", "p1", "Widget", "Acme", 100.0, 85.0, 60.0, 500.0, None, 0.0, 500.0, 25.0,
        );
        // net = 12500 - 12500 = 0
        assert_eq!(zero.recommendation, "INDIFFERENT");

        let neg = analyze_order_acceptance(
            "o4", "p1", "Widget", "Acme", 100.0, 85.0, 60.0, 500.0, None, 0.0, 600.0, 25.0,
        );
        assert_eq!(neg.recommendation, "REJECT");
        assert!(neg.conditions[0].contains("Net contribution negative"));
    }

    #[test]
    fn minimum_price_and_reject() {
        let d = analyze_order_acceptance(
            "o5",
            "p1",
            "Widget",
            "Acme",
            100.0,
            50.0,
            60.0,
            400.0,
            Some(90.0),
            1000.0,
            0.0,
            0.0,
        );
        assert_eq!(d.minimum_acceptable_price, 60.0 + 2.5);
        assert_eq!(d.recommendation, "REJECT");
        assert_eq!(d.full_cost_per_unit, Some(90.0));
        let zero_units = analyze_order_acceptance(
            "o6", "p", "W", "A", 100.0, 50.0, 60.0, 0.0, None, 100.0, 0.0, 0.0,
        );
        assert_eq!(zero_units.minimum_acceptable_price, 0.0);
    }

    #[test]
    fn one_time_order_matrix() {
        let spare_ok = analyze_one_time_order("W", 80.0, 60.0, 100.0, true);
        assert_eq!(spare_ok.recommendation, "ACCEPT");
        assert_eq!(
            spare_ok.reason,
            "Spare capacity available. Generates 2000.0 contribution."
        );
        let spare_bad = analyze_one_time_order("W", 50.0, 60.0, 100.0, true);
        assert_eq!(spare_bad.recommendation, "REJECT");
        let busy_ok = analyze_one_time_order("W", 80.0, 60.0, 100.0, false);
        assert_eq!(busy_ok.recommendation, "CONSIDER");
        let busy_bad = analyze_one_time_order("W", 50.0, 60.0, 100.0, false);
        assert_eq!(busy_bad.recommendation, "REJECT");
        assert_eq!(busy_bad.reason, "Price below variable cost");
        assert_eq!(spare_ok.minimum_acceptable_price, 60.0);
    }
}
