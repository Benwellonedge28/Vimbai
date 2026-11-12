//! IFRS 15 revenue from contracts with customers: the five-step model,
//! variable consideration, standalone selling price, progress measures,
//! customer options and contract modifications.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractDetails {
    pub contract_id: String,
    pub customer_id: String,
    pub contract_date: String,
    pub performance_obligation_ids: Vec<String>,
    pub transaction_price: f64,
    pub variable_consideration: f64,
    pub significant_financing_component: bool,
    pub non_cash_consideration: f64,
    pub consideration_payable_customer: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceObligation {
    pub obligation_id: String,
    pub description: String,
    /// "point_in_time" or "over_time".
    pub obligation_type: String,
    pub standalone_selling_price: f64,
    pub allocation_percentage: f64,
    pub satisfied: bool,
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
    pub transaction_price: f64,
    pub allocations: Vec<(String, f64)>,
    pub variable_consideration_allocated: f64,
    pub remaining_obligations: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Step4Satisfaction {
    pub obligation_id: String,
    pub method: String,
    pub progress_percentage: f64,
    pub satisfied_over_time: bool,
    pub revenue_to_recognize: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractAssetLiability {
    pub contract_id: String,
    pub contract_asset: f64,
    pub contract_liability: f64,
    pub net_position: f64,
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
    pub total_revenue_recognized: f64,
    pub total_contract_liabilities: f64,
    pub remaining_performance_obligations: f64,
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
    let mut total_revenue = 0.0;
    let mut total_liabilities = 0.0;
    let mut remaining_rpo = 0.0;

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
        let total_ssp: f64 = req
            .performance_obligations
            .iter()
            .filter(|po| in_contract(&po.obligation_id))
            .map(|po| po.standalone_selling_price)
            .sum();
        let mut allocations: Vec<(String, f64)> = Vec::new();
        for po in req
            .performance_obligations
            .iter()
            .filter(|po| in_contract(&po.obligation_id))
        {
            let alloc_pct = if total_ssp > 0.0 {
                po.standalone_selling_price / total_ssp
            } else {
                0.0
            };
            allocations.push((
                po.obligation_id.clone(),
                contract.transaction_price * alloc_pct,
            ));
        }
        step3.push(Step3PriceAllocation {
            contract_id: contract.contract_id.clone(),
            transaction_price: contract.transaction_price,
            allocations: allocations.clone(),
            variable_consideration_allocated: contract.variable_consideration,
            remaining_obligations: contract
                .performance_obligation_ids
                .iter()
                .filter(|id| progress_of(id).is_none())
                .cloned()
                .collect(),
        });

        // Step 4: recognise against progress.
        let mut contract_revenue = 0.0;
        for po in req
            .performance_obligations
            .iter()
            .filter(|po| in_contract(&po.obligation_id))
        {
            let progress = progress_of(&po.obligation_id).unwrap_or(0.0);
            let allocated = allocations
                .iter()
                .find(|(id, _)| *id == po.obligation_id)
                .map(|(_, v)| *v)
                .unwrap_or(0.0);

            let (revenue, remaining) = if po.obligation_type == "over_time" {
                (allocated * progress, allocated * (1.0 - progress))
            } else if progress == 1.0 {
                (allocated, 0.0)
            } else {
                (0.0, allocated)
            };

            remaining_rpo += remaining;
            contract_revenue += revenue;
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

        total_revenue += contract_revenue;
        total_liabilities += remaining_rpo;
        positions.push(ContractAssetLiability {
            contract_id: contract.contract_id.clone(),
            contract_asset: (contract_revenue - contract.transaction_price).max(0.0),
            contract_liability: (contract.transaction_price - contract_revenue).max(0.0),
            net_position: contract_revenue - contract.transaction_price,
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
    pub expected_value: f64,
    pub most_likely_amount: f64,
    pub method_used: String,
    pub pre_constraint: f64,
    pub constraint_applied: f64,
    pub constrained_estimate: f64,
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
    expected_value: f64,
    most_likely_amount: f64,
    constraint_applied: f64,
) -> VariableConsideration {
    let best_estimate = if variable_type == "sales_based" {
        most_likely_amount
    } else {
        expected_value
    };
    let constrained = best_estimate.min(best_estimate * constraint_applied);

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
        pre_constraint: best_estimate,
        constraint_applied,
        constrained_estimate: constrained,
        reversal_risk: if constrained == best_estimate {
            "low".to_string()
        } else {
            "constrained".to_string()
        },
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SspAllocation {
    pub obligation_id: String,
    pub observable_price: f64,
    pub adjusted_market_assessment: f64,
    pub expected_cost_plus_margin: f64,
    pub recommended_ssp: f64,
    pub allocation_percentage: f64,
    pub allocated_transaction_price: f64,
}

/// `/standalone-selling-price`. The adjusted market assessment is always
/// the recommended price; the observable price and the cost-plus figure
/// are returned untouched and never used. The allocation is the stated
/// percentage of the transaction price, not a proportional split.
pub fn calculate_ssp_allocation(
    obligation_id: &str,
    observable_price: f64,
    adjusted_market_assessment: f64,
    expected_cost_plus_margin: f64,
    allocation_percentage: f64,
    transaction_price: f64,
) -> SspAllocation {
    SspAllocation {
        obligation_id: obligation_id.to_string(),
        observable_price,
        adjusted_market_assessment,
        expected_cost_plus_margin,
        recommended_ssp: adjusted_market_assessment,
        allocation_percentage,
        allocated_transaction_price: transaction_price * (allocation_percentage / 100.0),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressMeasurement {
    pub obligation_id: String,
    pub method_used: String,
    pub progress_percentage: f64,
    pub revenue_to_recognize: f64,
    pub costs_incurred: f64,
    pub estimated_total_costs: f64,
    pub expected_profit: f64,
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
    costs_incurred: f64,
    total_estimated_costs: f64,
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
        let cost_progress = if total_estimated_costs > 0.0 {
            costs_incurred / total_estimated_costs
        } else {
            0.0
        };
        let resource_progress = if total_resources > 0.0 {
            resources_consumed / total_resources
        } else {
            0.0
        };
        (cost_progress + resource_progress) / 2.0
    };
    let progress_pct = progress.clamp(0.0, 1.0);

    ProgressMeasurement {
        obligation_id: obligation_id.to_string(),
        method_used: method.to_string(),
        progress_percentage: progress_pct * 100.0,
        revenue_to_recognize: progress_pct,
        costs_incurred,
        estimated_total_costs: total_estimated_costs,
        expected_profit: total_estimated_costs * progress_pct - costs_incurred,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomerOption {
    pub option_type: String,
    pub has_material_right: bool,
    pub is_performance_obligation: bool,
    pub standalone_selling_price: Option<f64>,
    pub present_value: Option<f64>,
    pub probability_adjusted_value: Option<f64>,
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
    standalone_selling_price: f64,
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
        standalone_selling_price * (1.0 - discount_rate)
    } else {
        standalone_selling_price
    };
    let exercise_probability = expected_exercise / 100.0;

    CustomerOption {
        option_type: option_type.to_string(),
        has_material_right: true,
        is_performance_obligation: true,
        standalone_selling_price: Some(standalone_selling_price),
        present_value: Some(pv_ssp),
        probability_adjusted_value: Some(pv_ssp * exercise_probability),
        treatment: "allocated to transaction price as material right".to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractModification {
    pub original_contract_id: String,
    pub modification_type: String,
    pub original_price: f64,
    pub new_price: f64,
    pub price_change: f64,
    pub additional_revenue: f64,
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
    original_price: f64,
    new_price: f64,
    original_progress: f64,
    modification_date: &str,
) -> ContractModification {
    let price_change = new_price - original_price;
    let scope_change = modification_type == "new_goods" || modification_type == "scope_change";

    let (additional_revenue, treatment) = if modification_type == "price_change" {
        (
            price_change * (1.0 - original_progress),
            "prospective - cumulative catch-up adjustment",
        )
    } else if scope_change && price_change >= 0.0 {
        (
            price_change,
            "prospective - modification creates new contract",
        )
    } else {
        (
            price_change,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn obligations() -> Vec<PerformanceObligation> {
        vec![
            PerformanceObligation {
                obligation_id: "po1".into(),
                description: "Software licence".into(),
                obligation_type: "point_in_time".into(),
                standalone_selling_price: 60_000.0,
                allocation_percentage: 60.0,
                satisfied: false,
                satisfaction_date: None,
            },
            PerformanceObligation {
                obligation_id: "po2".into(),
                description: "Installation".into(),
                obligation_type: "over_time".into(),
                standalone_selling_price: 40_000.0,
                allocation_percentage: 40.0,
                satisfied: false,
                satisfaction_date: None,
            },
        ]
    }

    fn contract(id: &str, price: f64) -> ContractDetails {
        ContractDetails {
            contract_id: id.into(),
            customer_id: "cust1".into(),
            contract_date: "2026-01-15".into(),
            performance_obligation_ids: vec!["po1".into(), "po2".into()],
            transaction_price: price,
            ..Default::default()
        }
    }

    #[test]
    fn allocation_is_proportional_and_ignores_variable_consideration() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            company_id: "c1".into(),
            reporting_date: "2026-12-31".into(),
            contracts: vec![ContractDetails {
                variable_consideration: 25_000.0,
                significant_financing_component: true,
                non_cash_consideration: 5_000.0,
                consideration_payable_customer: 3_000.0,
                ..contract("K1", 100_000.0)
            }],
            performance_obligations: obligations(),
            actual_progress: vec![],
        });
        let alloc = &r.step3_allocations[0];
        assert_eq!(alloc.allocations[0], ("po1".to_string(), 60_000.0));
        assert_eq!(alloc.allocations[1], ("po2".to_string(), 40_000.0));
        // echoed but never added to the allocatable pool
        assert_eq!(alloc.variable_consideration_allocated, 25_000.0);
        let allocated: f64 = alloc.allocations.iter().map(|(_, v)| *v).sum();
        assert_eq!(allocated, 100_000.0);
    }

    #[test]
    fn step_one_never_rejects_a_contract() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", 100_000.0)],
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
            contracts: vec![contract("K1", 100_000.0)],
            performance_obligations: obligations(),
            actual_progress: vec![("po1".into(), 1.0), ("po2".into(), 0.4)],
            ..Default::default()
        });
        // po1 complete: 60,000; po2 40% of 40,000: 16,000
        assert_eq!(r.total_revenue_recognized, 76_000.0);
        assert_eq!(r.remaining_performance_obligations, 24_000.0);
        assert_eq!(r.step4_satisfaction[0].revenue_to_recognize, 60_000.0);
        assert_eq!(r.step4_satisfaction[1].revenue_to_recognize, 16_000.0);
        // a 40%-complete over-time obligation still reports not satisfied
        assert!(!r.step4_satisfaction[1].satisfied_over_time);
        assert_eq!(r.step4_satisfaction[1].method, "input");
        assert_eq!(r.step4_satisfaction[0].method, "output");
    }

    #[test]
    fn point_in_time_partial_progress_recognises_nothing() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", 100_000.0)],
            performance_obligations: obligations(),
            actual_progress: vec![("po1".into(), 0.9)],
            ..Default::default()
        });
        assert_eq!(r.total_revenue_recognized, 0.0);
        assert_eq!(r.remaining_performance_obligations, 100_000.0);
        assert_eq!(r.step4_satisfaction[0].method, "output");
    }

    #[test]
    fn contract_position_is_a_liability_unless_progress_overruns_price() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", 100_000.0)],
            performance_obligations: obligations(),
            actual_progress: vec![("po1".into(), 1.0)],
            ..Default::default()
        });
        let pos = &r.contract_assets[0];
        assert_eq!(pos.contract_asset, 0.0);
        assert_eq!(pos.contract_liability, 40_000.0);
        assert_eq!(pos.net_position, -40_000.0);

        let overdone = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", 100_000.0)],
            performance_obligations: obligations(),
            actual_progress: vec![("po2".into(), 1.5)],
            ..Default::default()
        });
        assert_eq!(overdone.total_revenue_recognized, 60_000.0);
        assert_eq!(overdone.contract_assets[0].contract_asset, 0.0);
    }

    #[test]
    fn zero_standalone_prices_allocate_nothing() {
        let mut zeroed = obligations();
        for po in &mut zeroed {
            po.standalone_selling_price = 0.0;
        }
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", 100_000.0)],
            performance_obligations: zeroed,
            actual_progress: vec![("po1".into(), 1.0)],
            ..Default::default()
        });
        assert_eq!(r.step3_allocations[0].allocations[0].1, 0.0);
        assert_eq!(r.total_revenue_recognized, 0.0);
    }

    #[test]
    fn remaining_list_comes_from_presence_in_progress() {
        let r = recognize_revenue(&RevenueRecognitionRequest {
            contracts: vec![contract("K1", 100_000.0)],
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
            contracts: vec![contract("K1", 100_000.0), contract("K2", 50_000.0)],
            performance_obligations: obligations(),
            actual_progress: vec![],
            ..Default::default()
        });
        // each contract leaves its full price outstanding
        assert_eq!(r.remaining_performance_obligations, 150_000.0);
        // but the reported total adds the running figure twice over
        assert_eq!(r.total_contract_liabilities, 250_000.0);
        assert_ne!(
            r.total_contract_liabilities,
            r.remaining_performance_obligations
        );
    }

    #[test]
    fn variable_consideration_methods_and_constraint() {
        let sales = estimate_variable_consideration("K1", "sales_based", 90_000.0, 80_000.0, 0.8);
        assert_eq!(sales.pre_constraint, 80_000.0);
        assert_eq!(sales.method_used, "most_likely amount");
        assert_eq!(sales.constrained_estimate, 64_000.0);
        assert_eq!(sales.reversal_risk, "constrained");

        let milestone = estimate_variable_consideration("K1", "milestone", 90_000.0, 80_000.0, 1.0);
        assert_eq!(milestone.pre_constraint, 90_000.0);
        assert_eq!(milestone.method_used, "expected value");
        assert_eq!(milestone.constrained_estimate, 90_000.0);
        assert_eq!(milestone.reversal_risk, "low");

        // a factor above 1 cannot raise the estimate
        let generous = estimate_variable_consideration("K1", "royalty", 50_000.0, 10.0, 1.5);
        assert_eq!(generous.constrained_estimate, 50_000.0);
    }

    #[test]
    fn ssp_always_uses_the_adjusted_market_figure() {
        let s = calculate_ssp_allocation("po1", 55_000.0, 60_000.0, 48_000.0, 60.0, 100_000.0);
        assert_eq!(s.recommended_ssp, 60_000.0);
        assert_eq!(s.allocated_transaction_price, 60_000.0);
        assert_eq!(s.observable_price, 55_000.0);
        assert_eq!(s.expected_cost_plus_margin, 48_000.0);
    }

    #[test]
    fn output_progress_depends_on_the_word_units() {
        let with_units = measure_progress(
            "units_delivered",
            "output_methods",
            30.0,
            120.0,
            0.0,
            0.0,
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
            0.0,
            0.0,
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
            400_000.0,
            800_000.0,
            200_000.0,
            1_000_000.0,
        );
        // (0.5 + 0.2) / 2 = 0.35
        assert_eq!(p.progress_percentage, 35.0);
        assert_eq!(p.expected_profit, 800_000.0 * 0.35 - 400_000.0);
    }

    #[test]
    fn progress_is_clamped_and_guarded() {
        let over = measure_progress("po2", "input_methods", 0.0, 0.0, 900.0, 800.0, 200.0, 100.0);
        assert_eq!(over.progress_percentage, 100.0);
        let empty = measure_progress("po2", "input_methods", 0.0, 0.0, 10.0, 0.0, 5.0, 0.0);
        assert_eq!(empty.progress_percentage, 0.0);
        let unknown = measure_progress("po2", "whatever", 0.0, 0.0, 1.0, 2.0, 1.0, 2.0);
        assert_eq!(unknown.progress_percentage, 50.0);
    }

    #[test]
    fn customer_option_without_material_right_is_not_an_obligation() {
        let o = assess_customer_options("loyalty", false, 1_000.0, 0.1, 60.0);
        assert!(!o.is_performance_obligation);
        assert!(o.present_value.is_none());
        assert_eq!(
            o.treatment,
            "not a distinct good/service - no separate recognition"
        );
    }

    #[test]
    fn customer_option_material_right_is_discounted_then_weighted() {
        let o = assess_customer_options("future_purchase", true, 1_000.0, 0.2, 60.0);
        assert!(o.is_performance_obligation);
        assert_eq!(o.present_value, Some(800.0));
        assert_eq!(o.probability_adjusted_value, Some(480.0));
        assert_eq!(
            o.treatment,
            "allocated to transaction price as material right"
        );

        let no_discount = assess_customer_options("discount", true, 1_000.0, 0.0, 100.0);
        assert_eq!(no_discount.present_value, Some(1_000.0));
    }

    #[test]
    fn modification_treatments() {
        let price = account_for_modification(
            "K1",
            "price_change",
            100_000.0,
            120_000.0,
            0.4,
            "2026-05-01",
        );
        assert_eq!(price.additional_revenue, 12_000.0);
        assert_eq!(
            price.treatment,
            "prospective - cumulative catch-up adjustment"
        );

        let scope =
            account_for_modification("K1", "new_goods", 100_000.0, 140_000.0, 0.4, "2026-05-01");
        assert_eq!(scope.additional_revenue, 40_000.0);
        assert_eq!(
            scope.treatment,
            "prospective - modification creates new contract"
        );

        // scope change with a price CUT falls into the catch-up branch
        let cut =
            account_for_modification("K1", "scope_change", 100_000.0, 80_000.0, 0.4, "2026-05-01");
        assert_eq!(cut.price_change, -20_000.0);
        assert_eq!(cut.additional_revenue, -20_000.0);
        assert_eq!(
            cut.treatment,
            "cumulative catch-up - modification does not create new contract"
        );
    }
}
