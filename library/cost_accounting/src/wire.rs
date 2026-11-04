//! The cost-accounting endpoints, as the services answered them.
//!
//! Each of these is one endpoint the Python services published: the same payload in,
//! the same body out. The maths is in the modules beside this one; what is here is
//! only the shape of the request, including the defaults a client was allowed to
//! leave out. They live with the library rather than in a service so a route can
//! answer the endpoint after the service is retired.

use crate::activity_based::{calculate_abc, AbcRequest};
use crate::actual_cost::{
    actual_labour_cost, actual_material_cost, compare_actual_to_budget, total_actual_cost,
    variance_breakdown,
};
use crate::budgeted_cost::{
    compare_output, cost_breakdown, create_budgeted_cost, flexible_budget, total_budget, BudgetItem,
};
use crate::cost_centre::{generate_summary, CostCentreCost as SummaryCost};
use crate::job_costing::{add_cost, CostType, JobCosts};
use crate::limiting_factor::{
    analyze_limiting_factor, compare_products_for_factor, ProductWithFactor,
};
use crate::order_acceptance::{analyze_one_time_order, analyze_order_acceptance};
use crate::overhead_apportionment::{
    apportion_overhead, CostCentreDatum, OverheadItem as ApportionOverheadItem,
};
use crate::prime_cost::{direct_cost_item, prime_cost, prime_cost_from_items, DirectCostItem};
use crate::standard_cost::{
    labour_standard, material_standard, overhead_standard, standard_per_unit, total_standard,
    variance_analysis,
};
use crate::throughput::{analyze_throughput, ThroughputRequest};
use crate::total_production_cost::calculate_total_production_cost;
use serde::Deserialize;
use serde_json::{json, Value};

/// `abc_calculate` as JSON in, JSON out.
pub fn abc_calculate_from_json(payload: &Value) -> Result<Value, String> {
    let request: AbcRequest =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(calculate_abc(&request)).map_err(|e| e.to_string())
}

/// `throughput_accounting` as JSON in, JSON out.
pub fn throughput_accounting_from_json(payload: &Value) -> Result<Value, String> {
    let request: ThroughputRequest =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(analyze_throughput(&request)).map_err(|e| e.to_string())
}

/// `production_total_cost` as JSON in, JSON out.
pub fn production_total_cost_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        product_id: String,
        #[serde(default)]
        department_id: String,
        #[serde(default)]
        period: String,
        #[serde(default)]
        direct_materials: f64,
        #[serde(default)]
        direct_labor: f64,
        #[serde(default)]
        direct_expenses: f64,
        #[serde(default)]
        factory_rent: f64,
        #[serde(default)]
        factory_depreciation: f64,
        #[serde(default)]
        factory_insurance: f64,
        #[serde(default)]
        factory_maintenance: f64,
        #[serde(default)]
        other_overhead: f64,
        #[serde(default)]
        wip_opening: f64,
        #[serde(default)]
        wip_closing: f64,
        #[serde(default)]
        units_produced: i64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(calculate_total_production_cost(
        &b.product_id,
        &b.department_id,
        &b.period,
        b.direct_materials,
        b.direct_labor,
        b.direct_expenses,
        b.factory_rent,
        b.factory_depreciation,
        b.factory_insurance,
        b.factory_maintenance,
        b.other_overhead,
        b.wip_opening,
        b.wip_closing,
        b.units_produced,
    ))
    .map_err(|e| e.to_string())
}

/// `standard_material` as JSON in, JSON out.
pub fn standard_material_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        standard_price_per_unit: f64,
        #[serde(default)]
        standard_quantity: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(material_standard(
        b.standard_price_per_unit,
        b.standard_quantity,
    ))
    .map_err(|e| e.to_string())
}

/// `standard_labour` as JSON in, JSON out.
pub fn standard_labour_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        standard_rate_per_hour: f64,
        #[serde(default)]
        standard_hours: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(labour_standard(b.standard_rate_per_hour, b.standard_hours))
        .map_err(|e| e.to_string())
}

/// `standard_overhead` as JSON in, JSON out.
pub fn standard_overhead_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        overhead_rate: f64,
        #[serde(default)]
        standard_activity_level: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(overhead_standard(
        b.overhead_rate,
        b.standard_activity_level,
    ))
    .map_err(|e| e.to_string())
}

/// `standard_total` as JSON in, JSON out.
pub fn standard_total_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        standard_direct_material: f64,
        #[serde(default)]
        standard_direct_labour: f64,
        #[serde(default)]
        standard_overhead: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(total_standard(
        b.standard_direct_material,
        b.standard_direct_labour,
        b.standard_overhead,
    ))
    .map_err(|e| e.to_string())
}

/// `standard_per_unit` as JSON in, JSON out.
pub fn standard_per_unit_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        standard_material_cost: f64,
        #[serde(default)]
        standard_labour_cost: f64,
        #[serde(default)]
        standard_overhead: f64,
        #[serde(default)]
        units_produced: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(standard_per_unit(
        b.standard_material_cost,
        b.standard_labour_cost,
        b.standard_overhead,
        b.units_produced,
    ))
    .map_err(|e| e.to_string())
}

/// `standard_variance` as JSON in, JSON out.
pub fn standard_variance_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        standard_cost: f64,
        #[serde(default)]
        actual_cost: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(variance_analysis(b.standard_cost, b.actual_cost))
        .map_err(|e| e.to_string())
}

/// `actual_material` as JSON in, JSON out.
pub fn actual_material_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        actual_quantity: f64,
        #[serde(default)]
        actual_price_per_unit: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(actual_material_cost(
        b.actual_quantity,
        b.actual_price_per_unit,
    ))
    .map_err(|e| e.to_string())
}

/// `actual_labour` as JSON in, JSON out.
pub fn actual_labour_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        actual_hours: f64,
        #[serde(default)]
        actual_rate_per_hour: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(actual_labour_cost(b.actual_hours, b.actual_rate_per_hour))
        .map_err(|e| e.to_string())
}

/// `actual_total` as JSON in, JSON out.
pub fn actual_total_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        actual_direct_material: f64,
        #[serde(default)]
        actual_direct_labour: f64,
        #[serde(default)]
        actual_overhead: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(total_actual_cost(
        b.actual_direct_material,
        b.actual_direct_labour,
        b.actual_overhead,
    ))
    .map_err(|e| e.to_string())
}

/// `actual_compare_budget` as JSON in, JSON out.
pub fn actual_compare_budget_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        actual_cost: f64,
        #[serde(default)]
        budgeted_cost: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(compare_actual_to_budget(b.actual_cost, b.budgeted_cost))
        .map_err(|e| e.to_string())
}

/// `actual_variance` as JSON in, JSON out.
pub fn actual_variance_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        actual_material: f64,
        #[serde(default)]
        budgeted_material: f64,
        #[serde(default)]
        actual_labour: f64,
        #[serde(default)]
        budgeted_labour: f64,
        #[serde(default)]
        actual_overhead: f64,
        #[serde(default)]
        budgeted_overhead: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(variance_breakdown(
        b.actual_material,
        b.budgeted_material,
        b.actual_labour,
        b.budgeted_labour,
        b.actual_overhead,
        b.budgeted_overhead,
    ))
    .map_err(|e| e.to_string())
}

/// `prime_cost` as JSON in, JSON out.
pub fn prime_cost_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        product_id: String,
        #[serde(default)]
        period: String,
        #[serde(default)]
        direct_materials: f64,
        #[serde(default)]
        direct_labor: f64,
        #[serde(default)]
        direct_expenses: f64,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(prime_cost(
        &body.product_id,
        &body.period,
        body.direct_materials,
        body.direct_labor,
        body.direct_expenses,
    ))
    .map_err(|e| e.to_string())
}

/// `prime_cost_from_items` as JSON in, JSON out.
pub fn prime_cost_from_items_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(serde::Deserialize)]
    struct Item {
        #[serde(default)]
        item_name: String,
        #[serde(default)]
        item_type: String,
        #[serde(default)]
        amount: f64,
        #[serde(default = "one")]
        units: f64,
        #[serde(default)]
        product_id: Option<String>,
    }

    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        product_id: String,
        #[serde(default)]
        period: String,
        #[serde(default)]
        items: Vec<Item>,
    }

    fn one() -> f64 {
        1.0
    }

    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let items: Vec<DirectCostItem> = body
        .items
        .into_iter()
        .map(|i| {
            direct_cost_item(
                &i.item_name,
                &i.item_type,
                i.amount,
                i.units,
                i.product_id.as_deref(),
            )
        })
        .collect();
    serde_json::to_value(prime_cost_from_items(
        &body.product_id,
        &body.period,
        &items,
    ))
    .map_err(|e| e.to_string())
}

/// `cost_centre_summary` as JSON in, JSON out.
pub fn cost_centre_summary_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        cost_centre_id: String,
        period: String,
        #[serde(default)]
        allocated_service_costs: f64,
        #[serde(default)]
        costs: Vec<SummaryCost>,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(generate_summary(
        &b.costs,
        &b.cost_centre_id,
        &b.period,
        b.allocated_service_costs,
    ))
    .map_err(|e| e.to_string())
}

/// `overhead_apportion` as JSON in, JSON out.
pub fn overhead_apportion_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        overhead_id: String,
        overhead_name: String,
        total_amount: f64,
        basis: String,
        period: String,
        #[serde(default)]
        cost_centre_data: Vec<CostCentreDatumRaw>,
    }
    #[derive(serde::Deserialize)]
    struct CostCentreDatumRaw {
        cost_centre_id: String,
        cost_centre_name: String,
        #[serde(flatten)]
        basis_values: std::collections::BTreeMap<String, serde_json::Value>,
    }
    fn numbers(raw: Vec<CostCentreDatumRaw>) -> Vec<CostCentreDatum> {
        raw.into_iter()
            .map(|c| CostCentreDatum {
                cost_centre_id: c.cost_centre_id,
                cost_centre_name: c.cost_centre_name,
                basis_values: c
                    .basis_values
                    .into_iter()
                    .filter_map(|(k, v)| v.as_f64().map(|n| (k, n)))
                    .collect(),
            })
            .collect()
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let overhead = ApportionOverheadItem {
        overhead_id: b.overhead_id,
        overhead_name: b.overhead_name,
        total_amount: b.total_amount,
        basis: b.basis,
    };
    serde_json::to_value(apportion_overhead(
        &overhead,
        &b.period,
        &numbers(b.cost_centre_data),
    ))
    .map_err(|e| e.to_string())
}

/// `limiting_factor_analyze` as JSON in, JSON out.
pub fn limiting_factor_analyze_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        limiting_factor: String,
        #[serde(default)]
        factor_type: String,
        #[serde(default)]
        total_available: f64,
        #[serde(default)]
        products: Vec<ProductWithFactor>,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(analyze_limiting_factor(
        &body.limiting_factor,
        &body.factor_type,
        body.total_available,
        &body.products,
    ))
    .map_err(|e| e.to_string())
}

/// `limiting_factor_compare` as JSON in, JSON out.
pub fn limiting_factor_compare_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        products: Vec<ProductWithFactor>,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(serde_json::json!({
        "ranked_products": compare_products_for_factor(&body.products),
    }))
    .map_err(|e| e.to_string())
}

/// `order_acceptance` as JSON in, JSON out.
pub fn order_acceptance_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        order_id: String,
        #[serde(default)]
        product_id: String,
        #[serde(default)]
        product_name: String,
        #[serde(default)]
        customer_name: String,
        #[serde(default)]
        normal_selling_price: f64,
        #[serde(default)]
        offered_price: f64,
        #[serde(default)]
        variable_cost_per_unit: f64,
        #[serde(default)]
        units_requested: f64,
        #[serde(default)]
        full_cost_per_unit: Option<f64>,
        #[serde(default)]
        incremental_fixed_costs: f64,
        #[serde(default)]
        displaced_sales_units: f64,
        #[serde(default)]
        displaced_contribution_per_unit: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(analyze_order_acceptance(
        &b.order_id,
        &b.product_id,
        &b.product_name,
        &b.customer_name,
        b.normal_selling_price,
        b.offered_price,
        b.variable_cost_per_unit,
        b.units_requested,
        b.full_cost_per_unit,
        b.incremental_fixed_costs,
        b.displaced_sales_units,
        b.displaced_contribution_per_unit,
    ))
    .map_err(|e| e.to_string())
}

/// `one_time_order` as JSON in, JSON out.
pub fn one_time_order_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        product_name: String,
        #[serde(default)]
        offered_price: f64,
        #[serde(default)]
        variable_cost_per_unit: f64,
        #[serde(default)]
        units: f64,
        #[serde(default = "yes")]
        has_spare_capacity: bool,
    }
    fn yes() -> bool {
        true
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(analyze_one_time_order(
        &b.product_name,
        b.offered_price,
        b.variable_cost_per_unit,
        b.units,
        b.has_spare_capacity,
    ))
    .map_err(|e| e.to_string())
}

/// `job_add_cost` as JSON in, JSON out.
pub fn job_add_cost_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        materials_cost: f64,
        #[serde(default)]
        labor_cost: f64,
        #[serde(default)]
        overhead_cost: f64,
        #[serde(default)]
        subcontractor_cost: f64,
        #[serde(default)]
        contract_value: f64,
        #[serde(default)]
        cost_type: String,
        #[serde(default)]
        amount: f64,
        /// Stored totals, echoed untouched when the cost type is unknown.
        #[serde(default)]
        total_cost: Option<f64>,
        #[serde(default)]
        gross_profit: Option<f64>,
        #[serde(default)]
        gross_margin: Option<f64>,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    let job = JobCosts {
        materials_cost: b.materials_cost,
        labor_cost: b.labor_cost,
        overhead_cost: b.overhead_cost,
        subcontractor_cost: b.subcontractor_cost,
    };
    // Unknown cost types are recorded by the service but never aggregated, so
    // the totals stay exactly as they came in.
    let kind = match b.cost_type.as_str() {
        // the service spells this one in the plural
        "materials" => Some(CostType::Material),
        "labor" => Some(CostType::Labor),
        "overhead" => Some(CostType::Overhead),
        "subcontractor" => Some(CostType::Subcontractor),
        _ => None,
    };
    let (updated, totals) = match kind {
        Some(kind) => {
            let (updated, totals) = add_cost(&job, kind, b.amount, b.contract_value);
            (updated, Some(totals))
        }
        None => (job.clone(), None),
    };
    let total_cost = match &totals {
        Some(t) => t.total_cost,
        None => b.total_cost.unwrap_or(
            job.materials_cost + job.labor_cost + job.overhead_cost + job.subcontractor_cost,
        ),
    };
    let (gross_profit, gross_margin) = match &totals {
        Some(t) => (t.gross_profit, t.gross_margin),
        None => {
            let profit = b.gross_profit.unwrap_or(b.contract_value - total_cost);
            let margin = b
                .gross_margin
                .unwrap_or((profit / b.contract_value.max(1.0)) * 100.0);
            (profit, margin)
        }
    };
    serde_json::to_value(serde_json::json!({
        "aggregated": kind.is_some(),
        "materials_cost": updated.materials_cost,
        "labor_cost": updated.labor_cost,
        "overhead_cost": updated.overhead_cost,
        "subcontractor_cost": updated.subcontractor_cost,
        "total_cost": total_cost,
        "gross_profit": gross_profit,
        "gross_margin": gross_margin,
    }))
    .map_err(|e| e.to_string())
}

/// `budget_create` as JSON in, JSON out.
pub fn budget_create_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        department: String,
        #[serde(default)]
        period: String,
        #[serde(default)]
        direct_material: f64,
        #[serde(default)]
        direct_labour: f64,
        #[serde(default)]
        overhead: f64,
        #[serde(default)]
        other_costs: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(create_budgeted_cost(
        &b.department,
        &b.period,
        b.direct_material,
        b.direct_labour,
        b.overhead,
        b.other_costs,
    ))
    .map_err(|e| e.to_string())
}

/// `budget_flexible` as JSON in, JSON out.
pub fn budget_flexible_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        budgeted_cost_per_unit: f64,
        #[serde(default)]
        actual_output: f64,
        #[serde(default)]
        budgeted_output: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(flexible_budget(
        b.budgeted_cost_per_unit,
        b.actual_output,
        b.budgeted_output,
    ))
    .map_err(|e| e.to_string())
}

/// `budget_total` as JSON in, JSON out.
pub fn budget_total_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        budgeted_costs: Vec<BudgetItem>,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(total_budget(&b.budgeted_costs)).map_err(|e| e.to_string())
}

/// `budget_compare_output` as JSON in, JSON out.
pub fn budget_compare_output_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        budgeted_units: f64,
        #[serde(default)]
        actual_units: f64,
        #[serde(default)]
        budgeted_cost_per_unit: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(compare_output(
        b.budgeted_units,
        b.actual_units,
        b.budgeted_cost_per_unit,
    ))
    .map_err(|e| e.to_string())
}

/// `budget_breakdown` as JSON in, JSON out.
///
/// Percentages that do not add to 100 answer `{"error": ...}` in the body rather
/// than a failure status, which is what the endpoint always did.
pub fn budget_breakdown_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        total_budgeted_cost: f64,
        #[serde(default)]
        material_percent: f64,
        #[serde(default)]
        labour_percent: f64,
        #[serde(default)]
        overhead_percent: f64,
    }
    let b: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    Ok(
        match cost_breakdown(
            b.total_budgeted_cost,
            b.material_percent,
            b.labour_percent,
            b.overhead_percent,
        ) {
            Ok(v) => serde_json::to_value(v).map_err(|e| e.to_string())?,
            Err(message) => json!({ "error": message }),
        },
    )
}

/// `cost_of_capital_calculate` as JSON in, JSON out.
pub fn cost_of_capital_calculate_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        interest_rate: f64,
        #[serde(default)]
        risk_premium: f64,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(crate::cost_of_capital::calculate(
        body.interest_rate,
        body.risk_premium,
    ))
    .map_err(|e| e.to_string())
}

/// `cost_of_capital_from_percent` as JSON in, JSON out.
pub fn cost_of_capital_from_percent_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        interest_rate_percent: f64,
        #[serde(default)]
        risk_premium_percent: f64,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    serde_json::to_value(crate::cost_of_capital::from_percent(
        body.interest_rate_percent,
        body.risk_premium_percent,
    ))
    .map_err(|e| e.to_string())
}

/// `cost_of_capital_wacc`. A refusal is a 200 with an `error` body, which is what
/// the service answered, so the adapter keeps it a body rather than a status.
pub fn cost_of_capital_wacc_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        equity_value: f64,
        #[serde(default)]
        cost_of_equity: f64,
        #[serde(default)]
        debt_value: f64,
        #[serde(default)]
        cost_of_debt: f64,
        #[serde(default)]
        tax_rate: f64,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    match crate::cost_of_capital::wacc(
        body.equity_value,
        body.cost_of_equity,
        body.debt_value,
        body.cost_of_debt,
        body.tax_rate,
    ) {
        Ok(answer) => serde_json::to_value(answer).map_err(|e| e.to_string()),
        Err(message) => Ok(serde_json::json!({ "error": message })),
    }
}

/// `cost_of_capital_simple_rate`, refusal as an `error` body for the same reason.
pub fn cost_of_capital_simple_rate_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        principal: f64,
        #[serde(default)]
        amount_after: f64,
        #[serde(default)]
        years: i64,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    match crate::cost_of_capital::simple_rate(body.principal, body.amount_after, body.years) {
        Ok(answer) => serde_json::to_value(answer).map_err(|e| e.to_string()),
        Err(message) => Ok(serde_json::json!({ "error": message })),
    }
}

/// `cost_of_capital_compound_rate`, refusal as an `error` body.
pub fn cost_of_capital_compound_rate_from_json(payload: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Body {
        #[serde(default)]
        present_value: f64,
        #[serde(default)]
        future_value: f64,
        #[serde(default)]
        years: i64,
    }
    let body: Body =
        serde_json::from_value(payload.clone()).map_err(|e| format!("invalid request: {e}"))?;
    match crate::cost_of_capital::compound_rate(body.present_value, body.future_value, body.years) {
        Ok(answer) => serde_json::to_value(answer).map_err(|e| e.to_string()),
        Err(message) => Ok(serde_json::json!({ "error": message })),
    }
}

/// `cost_of_capital_standard_rates`, the service's fixed table.
pub fn cost_of_capital_list_from_json(_payload: &Value) -> Result<Value, String> {
    serde_json::to_value(serde_json::json!({
        "standard_rates": crate::cost_of_capital::standard_rates(),
    }))
    .map_err(|e| e.to_string())
}
