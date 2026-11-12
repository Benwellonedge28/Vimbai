//! IFRS 15 revenue from contracts with customers: the five-step model,
//! variable consideration, standalone selling price, progress measures,
//! customer options and contract modifications.
//!
//! Every amount is a `Money`: transaction prices, standalone selling prices, allocations, revenue
//! recognised, contract assets and liabilities, cost figures and modification prices. Rates,
//! percentages and progress stay ratios, which is what they are - a progress measure multiplies an
//! allocation, and a constraint factor caps an estimate.
//!
//! The proportional allocation is carried out as `price * ssp / total_ssp` in exact arithmetic rather
//! than as `price * (ssp / total_ssp)`, because the second form rounds the ratio before it ever reaches
//! the amount. The result is the same to the last cent for the ordinary case and does not drift for a
//! price a double cannot hold.
use serde::{Deserialize, Serialize};
use vimbai_common::Money;

fn rate(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
}

/// One amount divided by another as a plain ratio.
fn ratio(numerator: &Money, denominator: &Money) -> f64 {
    if denominator.is_zero() {
        return 0.0;
    }
    numerator.over(denominator).to_f64_lossy()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractDetails {
    pub contract_id: String,
    pub customer_id: String,
    #[serde(default)]
    pub contract_date: String,
    #[serde(default)]
    pub performance_obligation_ids: Vec<String>,
    pub transaction_price: Money,
    #[serde(default)]
    pub variable_consideration: Money,
    #[serde(default)]
    pub significant_financing_component: bool,
    #[serde(default)]
    pub non_cash_consideration: Money,
    #[serde(default)]
    pub consideration_payable_customer: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceObligation {
    pub obligation_id: String,
    pub description: String,
    /// "point_in_time" or "over_time".
    pub obligation_type: String,
    pub standalone_selling_price: Money,
    #[serde(default)]
    pub allocation_percentage: f64,
    #[serde(default)]
    pub satisfied: bool,
    #[serde(default)]
    pub satisfaction_date: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevenueRecognitionRequest {
    pub company_id: String,
    pub reporting_date: String,
    pub contracts: Vec<ContractDetails>,
    pub performance_obligations: Vec<PerformanceObligation>,
    /// Obligation id to progress, 0.0 to 1.0. Presence here also decides
    /// which obligations are reported as remaining.
    pub actual_progress: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Step1Identification {
    pub contract_id: String,
    pub contract_exists: bool,
    pub probability_of_collection: f64,
    pub enforceability: bool,
    pub has_commercial_substance: bool,
    pub approved: bool,
    pub reasons_for_rejection: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Step2Obligations {
    pub contract_id: String,
    pub distinct_goods_or_services: Vec<String>,
    pub distinct_in_series: bool,
    pub performance_obligations_identified: usize,
    pub bundled_obligations: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Step3PriceAllocation {
    pub contract_id: String,
    pub transaction_price: Money,
    pub allocations: Vec<(String, Money)>,
    pub variable_consideration_allocated: Money,
    pub remaining_obligations: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Step4Satisfaction {
    pub obligation_id: String,
    pub method: String,
    pub progress_percentage: f64,
    pub satisfied_over_time: bool,
    pub revenue_to_recognize: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractAssetLiability {
    pub contract_id: String,
    pub contract_asset: Money,
    pub contract_liability: Money,
    pub net_position: Money,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ifrs15Response {
    pub company_id: String,
    pub reporting_date: String,
    pub step1_analysis: Vec<Step1Identification>,
    pub step2_obligations: Vec<Step2Obligations>,
    pub step3_allocations: Vec<Step3PriceAllocation>,
    pub step4_satisfaction: Vec<Step4Satisfaction>,
    pub contract_assets: Vec<ContractAssetLiability>,
    pub total_revenue_recognized: Money,
    pub total_contract_liabilities: Money,
    pub remaining_performance_obligations: Money,
}

/// `/recognize`, the five-step model.
///
/// PYTHON QUIRKS, all preserved:
/// - step 1 is simulated: collection probability is fixed at 0.95 and
///   existence, enforceability and commercial substance always pass, so
///   no contract is ever rejected;
/// - step 3 allocates ONLY the transaction price, by standalone selling
///   price. Variable consideration, non-cash consideration, consideration
///   payable to a customer and any financing component are ignored, with
///   variable consideration merely echoed back;
/// - when the standalone prices total zero every allocation is zero;
/// - `satisfied_over_time` is true only when an over-time obligation is
///   ALSO at 100% progress, so a 40%-complete obligation reports false;
/// - the remaining obligation list is driven by whether the id appears in
///   actual progress at all, not by the satisfied flag;
/// - `total_contract_liabilities` adds a RUNNING total, so with several
///   contracts it double counts earlier contracts' remainders and does not
///   equal `remaining_performance_obligations`;
/// - the contract asset can only be positive when progress exceeds 100%.
pub fn recognize_revenue(req: &RevenueRecognitionRequest) -> Ifrs15Response {
    let mut step1 = Vec::new();
    let mut step2 = Vec::new();
    let mut step3 = Vec::new();
    let mut step4 = Vec::new();
    let mut positions = Vec::new();
    let mut total_revenue = Money::zero();
    let mut total_liabilities = Money::zero();
    let mut remaining_rpo = Money::zero();

    let progress_of = |id: &str| -> Option<f64> {
        req.actual_progress
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| *v)
    };

    for contract in &req.contracts {
        // Step 1: identification, always approved.
        let collection_prob = 0.95;
        step1.push(Step1Identification {
            contract_id: contract.contract_id.clone(),
            contract_exists: true,
            probability_of_collection: collection_prob,
            enforceability: true,
            has_commercial_substance: true,
            approved: collection_prob > 0.5,
            reasons_for_rejection: Vec::new(),
        });

        // Step 2: obligations named on the contract.
        let in_contract = |id: &str| contract.performance_obligation_ids.iter().any(|x| x == id);
        let distinct_services: Vec<String> = req
            .performance_obligations
            .iter()
            .filter(|po| in_contract(&po.obligation_id))
            .map(|po| po.description.clone())
            .collect();
        step2.push(Step2Obligations {
            contract_id: contract.contract_id.clone(),
            distinct_goods_or_services: distinct_services,
            distinct_in_series: false,
            performance_obligations_identified: contract.performance_obligation_ids.len(),
            bundled_obligations: Vec::new(),
        });

        // Step 3: proportional allocation of the transaction price only.
        let total_ssp = Money::sum(
            req.performance_obligations
                .iter()
                .filter(|po| in_contract(&po.obligation_id))
                .map(|po| &po.standalone_selling_price),
        );
        let in_scope: Vec<&PerformanceObligation> = req
            .performance_obligations
            .iter()
            .filter(|po| in_contract(&po.obligation_id))
            .collect();
        // Allocated by largest remainder to the cent, so the parts add back to the transaction price
        // instead of leaving a rounding residue nobody can book. All-zero standalone prices allocate
        // nothing, which is what the service did.
        let parts = if total_ssp.is_zero() || total_ssp.is_negative() {
            vec![Money::zero(); in_scope.len()]
        } else {
            let weights: Vec<Money> = in_scope
                .iter()
                .map(|po| po.standalone_selling_price.clone())
                .collect();
            contract.transaction_price.allocate(&weights, 2)
        };
        let allocations: Vec<(String, Money)> = in_scope
            .iter()
            .zip(parts)
            .map(|(po, part)| (po.obligation_id.clone(), part))
            .collect();
        step3.push(Step3PriceAllocation {
            contract_id: contract.contract_id.clone(),
            transaction_price: contract.transaction_price.clone(),
            allocations: allocations.clone(),
            variable_consideration_allocated: contract.variable_consideration.clone(),
            remaining_obligations: contract
                .performance_obligation_ids
                .iter()
                .filter(|id| progress_of(id).is_none())
                .cloned()
                .collect(),
        });

        // Step 4: recognise against progress.
        let mut contract_revenue = Money::zero();
        for po in req
            .performance_obligations
            .iter()
            .filter(|po| in_contract(&po.obligation_id))
        {
            let progress = progress_of(&po.obligation_id).unwrap_or(0.0);
            let allocated = allocations
                .iter()
                .find(|(id, _)| *id == po.obligation_id)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(Money::zero);

            // Revenue is money in a currency: it is published to the cent, banker's rounded once, so a
            // 40% progress on 40,000 reports 16,000.00 and not 16,000.000...
            let (revenue, remaining) = if po.obligation_type == "over_time" {
                (
                    allocated.times(&rate(progress)).round(2),
                    allocated.times(&rate(1.0 - progress)).round(2),
                )
            } else if progress == 1.0 {
                (allocated, Money::zero())
            } else {
                (Money::zero(), allocated)
            };

            remaining_rpo = remaining_rpo.plus(&remaining);
            contract_revenue = contract_revenue.plus(&revenue);
            step4.push(Step4Satisfaction {
                obligation_id: po.obligation_id.clone(),
                method: if po.obligation_type == "over_time" {
                    "input".to_string()
                } else {
                    "output".to_string()
                },
                progress_percentage: progress,
                satisfied_over_time: po.obligation_type == "over_time" && progress == 1.0,
                revenue_to_recognize: revenue,
            });
        }

        total_revenue = total_revenue.plus(&contract_revenue);
        total_liabilities = total_liabilities.plus(&remaining_rpo);
        let net_position = contract_revenue.minus(&contract.transaction_price);
        positions.push(ContractAssetLiability {
            contract_id: contract.contract_id.clone(),
            contract_asset: if net_position.is_negative() {
                Money::zero()
            } else {
                net_position.clone()
            },
            contract_liability: if net_position.is_negative() {
                net_position.times(&rate(-1.0))
            } else {
                Money::zero()
            },
            net_position,
        });
    }

    Ifrs15Response {
        company_id: req.company_id.clone(),
        reporting_date: req.reporting_date.clone(),
        step1_analysis: step1,
        step2_obligations: step2,
        step3_allocations: step3,
        step4_satisfaction: step4,
        contract_assets: positions,
        total_revenue_recognized: total_revenue,
        total_contract_liabilities: total_liabilities,
        remaining_performance_obligations: remaining_rpo,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariableConsideration {
    pub contract_id: String,
    pub variable_type: String,
    pub expected_value: Money,
    pub most_likely_amount: Money,
    pub method_used: String,
    pub pre_constraint: Money,
    pub constraint_applied: f64,
    pub constrained_estimate: Money,
    pub reversal_risk: String,
}

/// `/variable-consideration`.
///
/// PYTHON QUIRKS, preserved: the method choice is inverted relative to the
/// standard (a sales-based pool takes the most likely amount, everything
/// else the expected value); the constraint can only ever reduce the
/// estimate, so a factor above 1 does nothing; and `reversal_risk` reports
/// "constrained" rather than a risk level whenever the figure moved.
pub fn estimate_variable_consideration(
    contract_id: &str,
    variable_type: &str,
    expected_value: Money,
    most_likely_amount: Money,
    constraint_applied: f64,
) -> VariableConsideration {
    let best_estimate = if variable_type == "sales_based" {
        most_likely_amount.clone()
    } else {
        expected_value.clone()
    };
    let capped = best_estimate.times(&rate(constraint_applied));
    let constrained = if capped < best_estimate {
        capped
    } else {
        best_estimate.clone()
    };

    VariableConsideration {
        contract_id: contract_id.to_string(),
        variable_type: variable_type.to_string(),
        expected_value,
        most_likely_amount,
        method_used: if variable_type == "sales_based" {
            "most_likely amount".to_string()
        } else {
            "expected value".to_string()
        },
        reversal_risk: if constrained == best_estimate {
            "low".to_string()
        } else {
            "constrained".to_string()
        },
        pre_constraint: best_estimate,
        constraint_applied,
        constrained_estimate: constrained,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SspAllocation {
    pub obligation_id: String,
    pub observable_price: Money,
    pub adjusted_market_assessment: Money,
    pub expected_cost_plus_margin: Money,
    pub recommended_ssp: Money,
    pub allocation_percentage: f64,
    pub allocated_transaction_price: Money,
}

/// `/standalone-selling-price`. The adjusted market assessment is always
/// the recommended price; the observable price and the cost-plus figure
/// are returned untouched and never used. The allocation is the stated
/// percentage of the transaction price, not a proportional split.
pub fn calculate_ssp_allocation(
    obligation_id: &str,
    observable_price: Money,
    adjusted_market_assessment: Money,
    expected_cost_plus_margin: Money,
    allocation_percentage: f64,
    transaction_price: Money,
) -> SspAllocation {
    SspAllocation {
        obligation_id: obligation_id.to_string(),
        recommended_ssp: adjusted_market_assessment.clone(),
        allocated_transaction_price: transaction_price
            .times(&rate(allocation_percentage / 100.0))
            .round(2),
        observable_price,
        adjusted_market_assessment,
        expected_cost_plus_margin,
        allocation_percentage,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressMeasurement {
    pub obligation_id: String,
    pub method_used: String,
    pub progress_percentage: f64,
    /// The FRACTION of the obligation satisfied, not an amount.
    pub revenue_to_recognize: f64,
    pub costs_incurred: Money,
    pub estimated_total_costs: Money,
    pub expected_profit: Money,
}

/// `/progress-measurement`.
///
/// PYTHON QUIRKS, all preserved:
/// - the output method only uses delivered units when the obligation id
///   literally CONTAINS "units"; every other output obligation reports a
///   flat 50%;
/// - the input method averages cost progress with resource progress, so
///   the two must agree to reach 100%;
/// - the result is clamped to 0..1;
/// - `revenue_to_recognize` is the FRACTION, not an amount, while
///   `expected_profit` is really "allowance at current progress less costs
///   incurred".
#[allow(clippy::too_many_arguments)] // mirrors the FastAPI signature 1:1
pub fn measure_progress(
    obligation_id: &str,
    method: &str,
    units_delivered: f64,
    total_units: f64,
    costs_incurred: Money,
    total_estimated_costs: Money,
    resources_consumed: f64,
    total_resources: f64,
) -> ProgressMeasurement {
    let progress = if method == "output_methods" {
        if obligation_id.contains("units") {
            if total_units > 0.0 {
                units_delivered / total_units
            } else {
                0.0
            }
        } else {
            0.5
        }
    } else {
        ratio(&costs_incurred, &total_estimated_costs) * 0.5
            + (if total_resources > 0.0 {
                resources_consumed / total_resources
            } else {
                0.0
            }) * 0.5
    };
    let progress_pct = progress.clamp(0.0, 1.0);

    ProgressMeasurement {
        obligation_id: obligation_id.to_string(),
        method_used: method.to_string(),
        progress_percentage: progress_pct * 100.0,
        revenue_to_recognize: progress_pct,
        costs_incurred: costs_incurred.clone(),
        expected_profit: total_estimated_costs
            .times(&rate(progress_pct))
            .minus(&costs_incurred),
        estimated_total_costs: total_estimated_costs,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomerOption {
    pub option_type: String,
    pub has_material_right: bool,
    pub is_performance_obligation: bool,
    pub standalone_selling_price: Option<Money>,
    pub present_value: Option<Money>,
    pub probability_adjusted_value: Option<Money>,
    pub treatment: String,
}

/// `/customer-options`. Without a material right the answer carries only
/// the type, the flag and a treatment line; with one, the "present value"
/// is a plain discount of the standalone price and the allocation is that
/// figure times the exercise percentage divided by 100. No discounting is
/// applied despite the field name.
pub fn assess_customer_options(
    option_type: &str,
    has_material_right: bool,
    standalone_selling_price: Money,
    discount_rate: f64,
    expected_exercise: f64,
) -> CustomerOption {
    if !has_material_right {
        return CustomerOption {
            option_type: option_type.to_string(),
            has_material_right: false,
            is_performance_obligation: false,
            treatment: "not a distinct good/service - no separate recognition".to_string(),
            ..Default::default()
        };
    }

    let pv_ssp = if discount_rate > 0.0 {
        standalone_selling_price
            .times(&rate(1.0 - discount_rate))
            .round(2)
    } else {
        standalone_selling_price.clone()
    };
    let exercise_probability = expected_exercise / 100.0;

    CustomerOption {
        option_type: option_type.to_string(),
        has_material_right: true,
        is_performance_obligation: true,
        probability_adjusted_value: Some(pv_ssp.times(&rate(exercise_probability)).round(2)),
        present_value: Some(pv_ssp),
        standalone_selling_price: Some(standalone_selling_price),
        treatment: "allocated to transaction price as material right".to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractModification {
    pub original_contract_id: String,
    pub modification_type: String,
    pub original_price: Money,
    pub new_price: Money,
    pub price_change: Money,
    pub additional_revenue: Money,
    pub treatment: String,
    pub modification_date: String,
}

/// `/modification`.
///
/// PYTHON QUIRKS, preserved: a price change is recognised prospectively on
/// the unfinished share; a scope change with a non-negative price change
/// recognises the whole difference; everything else is labelled
/// "cumulative catch-up" but still books only the raw price change, and the
/// remaining-revenue figure computed for that branch is discarded.
/// A scope change with a price CUT therefore falls into the catch-up branch.
pub fn account_for_modification(
    original_contract_id: &str,
    modification_type: &str,
    original_price: Money,
    new_price: Money,
    original_progress: f64,
    modification_date: &str,
) -> ContractModification {
    let price_change = new_price.clone().minus(&original_price);
    let scope_change = modification_type == "new_goods" || modification_type == "scope_change";

    let (additional_revenue, treatment) = if modification_type == "price_change" {
        (
            price_change.times(&rate(1.0 - original_progress)).round(2),
            "prospective - cumulative catch-up adjustment",
        )
    } else if scope_change && !price_change.is_negative() {
        (
            price_change.clone(),
            "prospective - modification creates new contract",
        )
    } else {
        (
            price_change.clone(),
            "cumulative catch-up - modification does not create new contract",
        )
    };

    ContractModification {
        original_contract_id: original_contract_id.to_string(),
        modification_type: modification_type.to_string(),
        original_price,
        new_price,
        price_change,
        additional_revenue,
        treatment: treatment.to_string(),
        modification_date: modification_date.to_string(),
    }
}

// ------------------------------------------------------------- doors

/// Every door reads a flat payload, which is how the service's query parameters arrived: the API
/// surface merges the query string into the body before the calculator sees it.
fn parse<T: for<'de> Deserialize<'de>>(payload: &serde_json::Value) -> Result<T, String> {
    serde_json::from_value(payload.clone()).map_err(|error| format!("invalid request: {error}"))
}

fn to_value<T: Serialize>(value: &T) -> Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

/// `/recognize`.
pub fn recognize_revenue_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let request: RevenueRecognitionRequest = parse(payload)?;
    to_value(&recognize_revenue(&request))
}

/// `/variable-consideration`.
pub fn variable_consideration_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        contract_id: String,
        variable_type: String,
        expected_value: Money,
        most_likely_amount: Money,
        constraint_applied: f64,
    }
    let request: Request = parse(payload)?;
    to_value(&estimate_variable_consideration(
        &request.contract_id,
        &request.variable_type,
        request.expected_value,
        request.most_likely_amount,
        request.constraint_applied,
    ))
}

/// `/standalone-selling-price`.
pub fn ssp_allocation_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        obligation_id: String,
        observable_price: Money,
        adjusted_market_assessment: Money,
        expected_cost_plus_margin: Money,
        allocation_percentage: f64,
        transaction_price: Money,
    }
    let request: Request = parse(payload)?;
    to_value(&calculate_ssp_allocation(
        &request.obligation_id,
        request.observable_price,
        request.adjusted_market_assessment,
        request.expected_cost_plus_margin,
        request.allocation_percentage,
        request.transaction_price,
    ))
}

/// `/progress-measurement`.
pub fn progress_measurement_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        obligation_id: String,
        method: String,
        units_delivered: f64,
        total_units: f64,
        costs_incurred: Money,
        total_estimated_costs: Money,
        resources_consumed: f64,
        total_resources: f64,
    }
    let request: Request = parse(payload)?;
    to_value(&measure_progress(
        &request.obligation_id,
        &request.method,
        request.units_delivered,
        request.total_units,
        request.costs_incurred,
        request.total_estimated_costs,
        request.resources_consumed,
        request.total_resources,
    ))
}

/// `/customer-options`.
pub fn customer_options_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        option_type: String,
        has_material_right: bool,
        standalone_selling_price: Money,
        discount_rate: f64,
        expected_exercise: f64,
    }
    let request: Request = parse(payload)?;
    to_value(&assess_customer_options(
        &request.option_type,
        request.has_material_right,
        request.standalone_selling_price,
        request.discount_rate,
        request.expected_exercise,
    ))
}

/// `/modification`.
pub fn modification_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct Request {
        original_contract_id: String,
        modification_type: String,
        original_price: Money,
        new_price: Money,
        original_progress: f64,
        modification_date: String,
    }
    let request: Request = parse(payload)?;
    to_value(&account_for_modification(
        &request.original_contract_id,
        &request.modification_type,
        request.original_price,
        request.new_price,
        request.original_progress,
        &request.modification_date,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn fixed(amount: &Money) -> String {
        amount.to_fixed(2)
    }

    fn obligations() -> Vec<PerformanceObligation> {
        vec![
            PerformanceObligation {
                obligation_id: "po1".into(),
                description: "Software licence".into(),
                obligation_type: "point_in_time".into(),
                standalone_selling_price: m("60000"),
                allocation_percentage: 60.0,
                satisfied: false,
                satisfaction_date: None,
            },
            PerformanceObligation {
                obligation_id: "po2".into(),
                description: "Installation".into(),
                obligation_type: "over_time".into(),
                standalone_selling_price: m("40000"),
                allocation_percentage: 40.0,
                satisfied: false,
                satisfaction_date: None,
            },
        ]
    }

    fn contract(id: &str, price: &str) -> ContractDetails {
        ContractDetails {
            contract_id: id.into(),
            customer_id: "cust1".into(),
            contract_date: "2026-01-15".into(),
            performance_obligation_ids: vec!["po1".into(), "po2".into()],
            transaction_price: m(price),
            ..Default::default()
        }
    }

    #[test]
    fn allocation_is_proportional_and_ignores_variable_consideration() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            company_id: "c1".into(),
            reporting_date: "2026-12-31".into(),
            contracts: vec![ContractDetails {
                variable_consideration: m("25000"),
                significant_financing_component: true,
                non_cash_consideration: m("5000"),
                consideration_payable_customer: m("3000"),
                ..contract("K1", "100000")
            }],
            performance_obligations: obligations(),
            actual_progress: vec![],
        });
        let alloc = &r.step3_allocations[0];
        assert_eq!(fixed(&alloc.allocations[0].1), "60000.00");
        assert_eq!(fixed(&alloc.allocations[1].1), "40000.00");
        // echoed but never added to the allocatable pool
        assert_eq!(fixed(&alloc.variable_consideration_allocated), "25000.00");
        let allocated = Money::sum(alloc.allocations.iter().map(|(_, v)| v));
        assert_eq!(fixed(&allocated), "100000.00");
    }

    #[test]
    fn step_one_never_rejects_a_contract() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000")],
            performance_obligations: obligations(),
            ..Default::default()
        });
        let s1 = &r.step1_analysis[0];
        assert_eq!(s1.probability_of_collection, 0.95);
        assert!(s1.approved);
        assert!(s1.contract_exists && s1.enforceability && s1.has_commercial_substance);
        assert!(s1.reasons_for_rejection.is_empty());
    }

    #[test]
    fn overtime_recognises_by_progress_point_in_time_waits_for_completion() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000")],
            performance_obligations: obligations(),
            actual_progress: vec![("po1".into(), 1.0), ("po2".into(), 0.4)],
            ..Default::default()
        });
        // po1 complete: 60,000; po2 40% of 40,000: 16,000
        assert_eq!(fixed(&r.total_revenue_recognized), "76000.00");
        assert_eq!(fixed(&r.remaining_performance_obligations), "24000.00");
        assert_eq!(
            fixed(&r.step4_satisfaction[0].revenue_to_recognize),
            "60000.00"
        );
        assert_eq!(
            fixed(&r.step4_satisfaction[1].revenue_to_recognize),
            "16000.00"
        );
        // a 40%-complete over-time obligation still reports not satisfied
        assert!(!r.step4_satisfaction[1].satisfied_over_time);
        assert_eq!(r.step4_satisfaction[1].method, "input");
        assert_eq!(r.step4_satisfaction[0].method, "output");
    }

    #[test]
    fn point_in_time_partial_progress_recognises_nothing() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000")],
            performance_obligations: obligations(),
            actual_progress: vec![("po1".into(), 0.9)],
            ..Default::default()
        });
        assert_eq!(fixed(&r.total_revenue_recognized), "0.00");
        assert_eq!(fixed(&r.remaining_performance_obligations), "100000.00");
        assert_eq!(r.step4_satisfaction[0].method, "output");
    }

    #[test]
    fn contract_position_is_a_liability_unless_progress_overruns_price() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000")],
            performance_obligations: obligations(),
            actual_progress: vec![("po1".into(), 1.0)],
            ..Default::default()
        });
        let pos = &r.contract_assets[0];
        assert_eq!(fixed(&pos.contract_asset), "0.00");
        assert_eq!(fixed(&pos.contract_liability), "40000.00");
        assert_eq!(fixed(&pos.net_position), "-40000.00");

        let overdone = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000")],
            performance_obligations: obligations(),
            actual_progress: vec![("po2".into(), 1.5)],
            ..Default::default()
        });
        assert_eq!(fixed(&overdone.total_revenue_recognized), "60000.00");
        // the over-run is a liability of zero, not a negative one
        assert_eq!(
            fixed(&overdone.contract_assets[0].contract_liability),
            "40000.00"
        );
    }

    #[test]
    fn zero_standalone_prices_allocate_nothing() {
        let mut zeroed = obligations();
        for po in &mut zeroed {
            po.standalone_selling_price = Money::zero();
        }
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000")],
            performance_obligations: zeroed,
            actual_progress: vec![("po1".into(), 1.0)],
            ..Default::default()
        });
        assert_eq!(fixed(&r.step3_allocations[0].allocations[0].1), "0.00");
        assert_eq!(fixed(&r.total_revenue_recognized), "0.00");
    }

    #[test]
    fn remaining_list_comes_from_presence_in_progress() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000")],
            performance_obligations: obligations(),
            actual_progress: vec![("po1".into(), 0.2)],
            ..Default::default()
        });
        assert_eq!(
            r.step3_allocations[0].remaining_obligations,
            vec!["po2".to_string()]
        );
    }

    #[test]
    fn contract_liabilities_double_count_across_contracts() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "100000"), contract("K2", "50000")],
            performance_obligations: obligations(),
            actual_progress: vec![],
            ..Default::default()
        });
        // each contract leaves its full price outstanding
        assert_eq!(fixed(&r.remaining_performance_obligations), "150000.00");
        // but the reported total adds the running figure twice over
        assert_eq!(fixed(&r.total_contract_liabilities), "250000.00");
        assert_ne!(
            fixed(&r.total_contract_liabilities),
            fixed(&r.remaining_performance_obligations)
        );
    }

    #[test]
    fn a_transaction_price_past_double_precision_allocates_every_cent() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", "123456789012345678901.55")],
            performance_obligations: obligations(),
            actual_progress: vec![],
            ..Default::default()
        });
        let alloc = &r.step3_allocations[0];
        // 60% and 40% of a price no double can hold
        assert_eq!(fixed(&alloc.allocations[0].1), "74074073407407407340.93");
        assert_eq!(fixed(&alloc.allocations[1].1), "49382715604938271560.62");
        let allocated = Money::sum(alloc.allocations.iter().map(|(_, v)| v));
        // the parts add back to the price to the cent
        assert_eq!(fixed(&allocated), "123456789012345678901.55");
    }

    #[test]
    fn variable_consideration_methods_and_constraint() {
        let sales =
            estimate_variable_consideration("K1", "sales_based", m("90000"), m("80000"), 0.8);
        assert_eq!(fixed(&sales.pre_constraint), "80000.00");
        assert_eq!(sales.method_used, "most_likely amount");
        assert_eq!(fixed(&sales.constrained_estimate), "64000.00");
        assert_eq!(sales.reversal_risk, "constrained");

        let milestone =
            estimate_variable_consideration("K1", "milestone", m("90000"), m("80000"), 1.0);
        assert_eq!(fixed(&milestone.pre_constraint), "90000.00");
        assert_eq!(milestone.method_used, "expected value");
        assert_eq!(fixed(&milestone.constrained_estimate), "90000.00");
        assert_eq!(milestone.reversal_risk, "low");

        // a factor above 1 cannot raise the estimate
        let generous = estimate_variable_consideration("K1", "royalty", m("50000"), m("10"), 1.5);
        assert_eq!(fixed(&generous.constrained_estimate), "50000.00");
    }

    #[test]
    fn ssp_always_uses_the_adjusted_market_figure() {
        let s =
            calculate_ssp_allocation("po1", m("55000"), m("60000"), m("48000"), 60.0, m("100000"));
        assert_eq!(fixed(&s.recommended_ssp), "60000.00");
        assert_eq!(fixed(&s.allocated_transaction_price), "60000.00");
        assert_eq!(fixed(&s.observable_price), "55000.00");
        assert_eq!(fixed(&s.expected_cost_plus_margin), "48000.00");
    }

    #[test]
    fn output_progress_depends_on_the_word_units() {
        let with_units = measure_progress(
            "units_delivered",
            "output_methods",
            30.0,
            120.0,
            Money::zero(),
            Money::zero(),
            0.0,
            0.0,
        );
        assert_eq!(with_units.progress_percentage, 25.0);
        assert_eq!(with_units.revenue_to_recognize, 0.25);

        // no "units" in the id: flat 50% regardless of the figures
        let no_units = measure_progress(
            "milestone_1",
            "output_methods",
            90.0,
            100.0,
            Money::zero(),
            Money::zero(),
            0.0,
            0.0,
        );
        assert_eq!(no_units.progress_percentage, 50.0);
    }

    #[test]
    fn input_progress_averages_costs_and_resources() {
        let p = measure_progress(
            "po2",
            "input_methods",
            0.0,
            0.0,
            m("400000"),
            m("800000"),
            200_000.0,
            1_000_000.0,
        );
        // (0.5 + 0.2) / 2 = 0.35
        assert_eq!(p.progress_percentage, 35.0);
        assert_eq!(fixed(&p.expected_profit), "-120000.00");
    }

    #[test]
    fn progress_is_clamped_and_guarded() {
        let over = measure_progress(
            "po2",
            "input_methods",
            0.0,
            0.0,
            m("900"),
            m("800"),
            200.0,
            100.0,
        );
        assert_eq!(over.progress_percentage, 100.0);
        let empty = measure_progress(
            "po2",
            "input_methods",
            0.0,
            0.0,
            m("10"),
            Money::zero(),
            5.0,
            0.0,
        );
        assert_eq!(empty.progress_percentage, 0.0);
        let unknown = measure_progress("po2", "whatever", 0.0, 0.0, m("1"), m("2"), 1.0, 2.0);
        assert_eq!(unknown.progress_percentage, 50.0);
    }

    #[test]
    fn customer_option_without_material_right_is_not_an_obligation() {
        let o = assess_customer_options("loyalty", false, m("1000"), 0.1, 60.0);
        assert!(!o.is_performance_obligation);
        assert!(o.present_value.is_none());
        assert_eq!(
            o.treatment,
            "not a distinct good/service - no separate recognition"
        );
    }

    #[test]
    fn customer_option_material_right_is_discounted_then_weighted() {
        let o = assess_customer_options("future_purchase", true, m("1000"), 0.2, 60.0);
        assert!(o.is_performance_obligation);
        assert_eq!(
            o.present_value.as_ref().map(fixed),
            Some("800.00".to_string())
        );
        assert_eq!(
            o.probability_adjusted_value.as_ref().map(fixed),
            Some("480.00".to_string())
        );
        assert_eq!(
            o.treatment,
            "allocated to transaction price as material right"
        );

        let no_discount = assess_customer_options("discount", true, m("1000"), 0.0, 100.0);
        assert_eq!(
            no_discount.present_value.as_ref().map(fixed),
            Some("1000.00".to_string())
        );
    }

    #[test]
    fn modification_treatments() {
        let price = account_for_modification(
            "K1",
            "price_change",
            m("100000"),
            m("120000"),
            0.4,
            "2026-05-01",
        );
        assert_eq!(fixed(&price.additional_revenue), "12000.00");
        assert_eq!(
            price.treatment,
            "prospective - cumulative catch-up adjustment"
        );

        let scope = account_for_modification(
            "K1",
            "new_goods",
            m("100000"),
            m("140000"),
            0.4,
            "2026-05-01",
        );
        assert_eq!(fixed(&scope.additional_revenue), "40000.00");
        assert_eq!(
            scope.treatment,
            "prospective - modification creates new contract"
        );

        // scope change with a price CUT falls into the catch-up branch
        let cut = account_for_modification(
            "K1",
            "scope_change",
            m("100000"),
            m("80000"),
            0.4,
            "2026-05-01",
        );
        assert_eq!(fixed(&cut.price_change), "-20000.00");
        assert_eq!(fixed(&cut.additional_revenue), "-20000.00");
        assert_eq!(
            cut.treatment,
            "cumulative catch-up - modification does not create new contract"
        );
    }

    #[test]
    fn the_recognize_door_reads_the_published_shape() {
        let payload = serde_json::json!({
            "company_id": "c1",
            "reporting_date": "2026-12-31",
            "contracts": [{
                "contract_id": "K1",
                "customer_id": "cust1",
                "contract_date": "2026-01-15",
                "performance_obligation_ids": ["po1", "po2"],
                "transaction_price": "100000"
            }],
            "performance_obligations": [
                {"obligation_id": "po1", "description": "Licence",
                 "obligation_type": "point_in_time", "standalone_selling_price": "60000",
                 "allocation_percentage": 60.0, "satisfied": false},
                {"obligation_id": "po2", "description": "Installation",
                 "obligation_type": "over_time", "standalone_selling_price": "40000",
                 "allocation_percentage": 40.0, "satisfied": false}
            ],
            "actual_progress": [["po1", 1.0], ["po2", 0.4]]
        });
        let out = recognize_revenue_from_json(&payload).expect("door");
        assert_eq!(out["total_revenue_recognized"], "76000.00");
        assert_eq!(out["step3_allocations"][0]["allocations"][0][1], "60000.00");
    }

    #[test]
    fn an_unreadable_amount_is_refused_not_treated_as_zero() {
        let payload = serde_json::json!({
            "contract_id": "K1",
            "variable_type": "milestone",
            "expected_value": "not a number",
            "most_likely_amount": "10",
            "constraint_applied": 0.8
        });
        let error = variable_consideration_from_json(&payload).expect_err("refused");
        assert!(error.contains("invalid request"), "{error}");
    }
}
