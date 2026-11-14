#![forbid(unsafe_code)]
//! HTTP surface for the Vimbai calculation library.
//!
//! Every calculator in this crate is a pure function of its JSON payload, and every
//! one of them runs the same code the Python seam binds to. The point of the service
//! is to let the Python services retire: once a calculator answers here, the FastAPI
//! service in front of it holds nothing but routing.
//!
//! Money is carried as a decimal string wherever a figure is an amount, so no amount
//! is rounded on its way through the wire.

use axum::{
    extract::{Path, RawQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub mod accounting_book;
pub mod accounting_standards;
pub mod identity;
pub mod outbound;
pub mod records;
pub mod sockets;

pub type Calculator = fn(&Value) -> Result<Value, String>;

/// The registered calculators, by the name clients call them.
pub fn calculators() -> BTreeMap<&'static str, Calculator> {
    let mut map: BTreeMap<&'static str, Calculator> = BTreeMap::new();
    map.insert(
        "internal-control-evaluation",
        (|payload| Ok(advisory::evaluate_controls(payload))) as Calculator,
    );
    map.insert("target-costing", cost_accounting::target_cost::evaluate);
    map.insert("divestiture", group_accounting::divestiture::evaluate);
    map.insert(
        "deal-structuring-structure",
        capital_valuation::deal_structuring_from_json,
    );
    map.insert(
        "post-merger-track",
        capital_valuation::post_merger_from_json,
    );
    map.insert(
        "synergy-analysis-analyze",
        capital_valuation::synergy_analysis_from_json,
    );
    map.insert(
        "merger-valuation-value",
        capital_valuation::merger_from_json,
    );
    map.insert(
        "scenario-budget-analyze",
        budgeting_services::scenario_budget::scenario_budget_from_json,
    );
    map.insert(
        "profit-loss-prepare",
        budgeting_services::profit_loss::profit_loss_prepare_from_json,
    );
    map.insert(
        "profit-loss-compare",
        budgeting_services::profit_loss::profit_loss_compare_from_json,
    );
    map.insert(
        "suspense-error-analyze",
        accounting::suspense::suspense_error_from_json,
    );
    map.insert("kpi-tracking-track", reporting::kpi_tracking_from_json);
    map.insert("eps", ratios_statements::eps_from_json);
    map.insert(
        "payroll-accounting-process",
        equity_payroll::process_payroll_from_json,
    );
    map.insert(
        "business-valuation",
        (|payload| {
            let mut owned = payload.clone();
            if owned.get("valuation_date").is_none() {
                // The service stamped today's date; the calculator stays clockless, so the surface
                // supplies it. A stored valuation passes its own date and keeps it.
                let today = chrono::Utc::now()
                    .date_naive()
                    .format("%Y-%m-%d")
                    .to_string();
                if let Some(object) = owned.as_object_mut() {
                    object.insert("valuation_date".into(), Value::String(today));
                }
            }
            capital_valuation::value_business_from_json(&owned)
        }) as Calculator,
    );
    map.insert("business-valuation-wacc", capital_valuation::wacc_from_json);
    map.insert("business-valuation-capm", capital_valuation::capm_from_json);
    map.insert(
        "business-valuation-precedent-transaction",
        capital_valuation::precedent_transaction_from_json,
    );
    map.insert(
        "business-valuation-book-value",
        capital_valuation::book_value_from_json,
    );
    map.insert(
        "financial-statements-prepare",
        ratios_statements::prepare_statements_from_json,
    );
    map.insert("ifrs-15-recognize", ifrs::recognize_revenue_from_json);
    map.insert(
        "ifrs-15-variable-consideration",
        ifrs::variable_consideration_from_json,
    );
    map.insert(
        "ifrs-15-standalone-selling-price",
        ifrs::ssp_allocation_from_json,
    );
    map.insert(
        "ifrs-15-progress-measurement",
        ifrs::progress_measurement_from_json,
    );
    map.insert("ifrs-15-customer-options", ifrs::customer_options_from_json);
    map.insert("ifrs-15-modification", ifrs::modification_from_json);
    map.insert("ifrs-2-calculate", ifrs::share_based_calculate_from_json);
    map.insert(
        "ifrs-2-fair-value-equity-instruments",
        ifrs::instrument_fair_value_from_json,
    );
    map.insert(
        "ifrs-2-performance-condition",
        ifrs::performance_condition_from_json,
    );
    map.insert("ifrs-2-modification", ifrs::grant_modification_from_json);
    map.insert(
        "ifrs-2-group-transaction",
        ifrs::group_transaction_from_json,
    );
    map.insert("ifrs-9-classify", ifrs::classify_from_json);
    map.insert("ifrs-9-sppi-test", ifrs::sppi_test_from_json);
    map.insert("ifrs-9-business-model", ifrs::business_model_from_json);
    map.insert("ifrs-9-ecl-general", ifrs::ecl_general_from_json);
    map.insert("ifrs-9-ecl-simplified", ifrs::ecl_simplified_from_json);
    map.insert(
        "ifrs-9-hedge-effectiveness",
        ifrs::hedge_effectiveness_from_json,
    );
    map.insert(
        "lease-management-calculate",
        ifrs::lease_calculate_from_json,
    );
    map.insert("lease-management-payment", ifrs::lease_payment_from_json);
    map.insert(
        "statutory-filing-assess",
        tax::statutory_filing_assess_from_json,
    );
    map.insert(
        "tax-return-preparation-prepare",
        tax::tax_return_preparation_from_json,
    );
    map.insert(
        "business-documents-generate",
        reporting::business_documents_generate_from_json,
    );
    map.insert(
        "business-documents-generate-batch",
        reporting::business_documents_generate_batch_from_json,
    );
    map.insert(
        "management-accounts-prepare",
        ratios_statements::management_accounts_from_json,
    );
    map.insert(
        "management-dashboard-generate",
        ratios_statements::management_dashboard_from_json,
    );
    map.insert(
        "financial-planning-plan",
        ratios_statements::financial_planning_from_json,
    );
    map.insert(
        "operational-risk-assess",
        market_risk::operational_risk_from_json,
    );
    map.insert("croic", ratios_statements::croic_from_json);
    map.insert("amortization", assets::amortization_from_json);
    map.insert("depreciation", assets::depreciation_batch_from_json);
    map.insert("depreciation-method", assets::depreciation_method_from_json);
    map.insert("fixed-assets", assets::analyze_fixed_assets_from_json);
    map.insert("goodwill", assets::impairment_test_from_json);
    map.insert(
        "investment-property-fair-value-model",
        assets::fair_value_model_from_json,
    );
    map.insert(
        "investment-property-cost-model",
        assets::cost_model_from_json,
    );
    map.insert("investment-property-transfer", assets::transfer_from_json);
    map.insert(
        "investment-property-fair-value-disclosure",
        assets::disclosure_from_json,
    );
    map.insert("investment-property-yields", assets::yields_from_json);
    map.insert("biological-assets-measure", assets::measure_from_json);
    map.insert(
        "biological-assets-fair-value-level1",
        assets::level1_from_json,
    );
    map.insert(
        "biological-assets-fair-value-level2",
        assets::level2_from_json,
    );
    map.insert(
        "biological-assets-fair-value-level3",
        assets::level3_from_json,
    );
    map.insert(
        "biological-assets-consumable-bearer-distinction",
        assets::biological_classify_from_json,
    );
    map.insert(
        "biological-assets-disclosure-template",
        assets::biological_disclosure_from_json,
    );
    map.insert(
        "disposal-account-calculate",
        assets::disposal_calculate_from_json,
    );
    map.insert("disposal-account-post", assets::disposal_post_from_json);
    map.insert(
        "inventory-valuation",
        performance_measures::inventory_from_json,
    );
    map.insert("eva", ratios_statements::eva_from_json);
    // the endpoint's own payload: parent figures, nested subsidiary accounts, and
    // the presentation the service published around the consolidated core
    map.insert(
        "consolidation",
        group_accounting::consolidation::consolidate_statement_from_json,
    );
    map.insert(
        "options-pricing",
        market_risk::options::pricing_statement_from_json,
    );
    // the cost-accounting endpoints, each named after the service and the path the
    // service published it on
    map.insert(
        "activity-based-costing-calculate",
        cost_accounting::wire::abc_calculate_from_json,
    );
    map.insert(
        "throughput-accounting-analyze",
        cost_accounting::wire::throughput_accounting_from_json,
    );
    map.insert(
        "standard-cost-material-standard",
        cost_accounting::wire::standard_material_from_json,
    );
    map.insert(
        "standard-cost-labour-standard",
        cost_accounting::wire::standard_labour_from_json,
    );
    map.insert(
        "standard-cost-overhead-standard",
        cost_accounting::wire::standard_overhead_from_json,
    );
    map.insert(
        "standard-cost-total-standard",
        cost_accounting::wire::standard_total_from_json,
    );
    map.insert(
        "standard-cost-per-unit",
        cost_accounting::wire::standard_per_unit_from_json,
    );
    map.insert(
        "standard-cost-variance-analysis",
        cost_accounting::wire::standard_variance_from_json,
    );
    map.insert(
        "actual-cost-material-actual",
        cost_accounting::wire::actual_material_from_json,
    );
    map.insert(
        "actual-cost-labour-actual",
        cost_accounting::wire::actual_labour_from_json,
    );
    map.insert(
        "actual-cost-total-actual",
        cost_accounting::wire::actual_total_from_json,
    );
    map.insert(
        "actual-cost-compare-to-budget",
        cost_accounting::wire::actual_compare_budget_from_json,
    );
    map.insert(
        "actual-cost-variance-breakdown",
        cost_accounting::wire::actual_variance_from_json,
    );
    map.insert(
        "budgeted-cost-create",
        cost_accounting::wire::budget_create_from_json,
    );
    map.insert(
        "budgeted-cost-flexible-budget",
        cost_accounting::wire::budget_flexible_from_json,
    );
    map.insert(
        "budgeted-cost-total-budget",
        cost_accounting::wire::budget_total_from_json,
    );
    map.insert(
        "budgeted-cost-compare-output",
        cost_accounting::wire::budget_compare_output_from_json,
    );
    map.insert(
        "budgeted-cost-cost-breakdown",
        cost_accounting::wire::budget_breakdown_from_json,
    );
    map.insert(
        "prime-cost-calculate",
        cost_accounting::wire::prime_cost_from_json,
    );
    map.insert(
        "prime-cost-calculate-with-items",
        cost_accounting::wire::prime_cost_from_items_from_json,
    );
    map.insert(
        "cost-centre-summary-generate",
        cost_accounting::wire::cost_centre_summary_from_json,
    );
    map.insert(
        "overhead-apportionment-apportion",
        cost_accounting::wire::overhead_apportion_from_json,
    );
    map.insert(
        "limiting-factor-analyze",
        cost_accounting::wire::limiting_factor_analyze_from_json,
    );
    map.insert(
        "limiting-factor-compare-products",
        cost_accounting::wire::limiting_factor_compare_from_json,
    );
    map.insert(
        "order-acceptance-analyze",
        cost_accounting::wire::order_acceptance_from_json,
    );
    map.insert(
        "consolidation-eliminations",
        group_accounting::consolidation_views::eliminations_from_json,
    );
    map.insert(
        "consolidation-nci-calculation",
        group_accounting::consolidation_views::nci_from_json,
    );
    map.insert(
        "consolidation-goodwill-calculation",
        group_accounting::consolidation_views::goodwill_calculation_from_json,
    );
    map.insert(
        "consolidation-currency-translation",
        group_accounting::consolidation_views::currency_translation_from_json,
    );
    map.insert(
        "consolidation-validate",
        group_accounting::consolidation_views::validate_from_json,
    );
    map.insert(
        "corporate-tax-calculate",
        tax::statutory::statutory_from_json,
    );
    map.insert(
        "corporate-tax-compute",
        tax::statutory::compute_tax_from_json,
    );
    map.insert(
        "regulatory-ratio-calculate",
        bridge::http::regulatory_ratio_from_json,
    );
    map.insert("tax-risk-assess", bridge::http::tax_risk_from_json);
    map.insert("group-tax-consolidate", bridge::http::group_tax_from_json);
    map.insert(
        "vat-reporting-prepare-return",
        bridge::http::vat_return_from_json,
    );
    map.insert("government-grants-recognize", bridge::http::grant_from_json);
    map.insert(
        "balanced-scorecard-score",
        bridge::http::scorecard_from_json,
    );
    map.insert(
        "financial-distress-prediction-predict",
        bridge::http::distress_from_json,
    );
    map.insert(
        "fraud-risk-assessment-assess",
        bridge::http::fraud_risk_from_json,
    );
    map.insert(
        "pension-accounting-calculate",
        bridge::http::pension_from_json,
    );
    map.insert(
        "partnership-accounting-allocate",
        bridge::http::partnership_allocation_from_json,
    );
    map.insert(
        "partnership-sale-goodwill",
        bridge::http::partnership_goodwill_from_json,
    );
    map.insert(
        "partnership-sale-retirement",
        bridge::http::partner_buyout_from_json,
    );
    map.insert(
        "cost-accounting-job-cost",
        cost_accounting::wire::cost_accounting_job_cost_from_json,
    );
    map.insert(
        "cost-accounting-standards",
        cost_accounting::wire::cost_accounting_standards_from_json,
    );
    map.insert(
        "cost-of-capital-calculate",
        cost_accounting::wire::cost_of_capital_calculate_from_json,
    );
    map.insert(
        "cost-of-capital-from-percent",
        cost_accounting::wire::cost_of_capital_from_percent_from_json,
    );
    map.insert(
        "cost-of-capital-wacc",
        cost_accounting::wire::cost_of_capital_wacc_from_json,
    );
    map.insert(
        "cost-of-capital-simple-rate",
        cost_accounting::wire::cost_of_capital_simple_rate_from_json,
    );
    map.insert(
        "cost-of-capital-compound-rate",
        cost_accounting::wire::cost_of_capital_compound_rate_from_json,
    );
    map.insert(
        "cost-of-capital-list",
        cost_accounting::wire::cost_of_capital_list_from_json,
    );
    map.insert(
        "order-acceptance-one-time-order",
        cost_accounting::wire::one_time_order_from_json,
    );

    map.insert("du-pont", ratios_statements::du_pont_from_json);
    map.insert(
        "asset-turnover",
        ratios_statements::asset_turnover_from_json,
    );
    map.insert(
        "capital-expenditure-budget",
        budgeting::capex::prepare_from_json,
    );
    map.insert(
        "substantive-testing",
        advisory::substantive_testing::evaluate,
    );
    map.insert(
        "payment-optimization",
        treasury_fx::payment_optimization::evaluate,
    );
    map.insert(
        "net-realizable-value",
        performance_measures::realizable::net_realizable_value_from_json,
    );
    map.insert(
        "nrv-scrap-value",
        performance_measures::realizable::scrap_value_from_json,
    );
    map.insert(
        "nrv-residual-value",
        performance_measures::realizable::residual_value_from_json,
    );
    map.insert(
        "nrv-inventory",
        performance_measures::realizable::inventory_nrv_from_json,
    );
    map.insert(
        "nrv-compare-cost",
        performance_measures::realizable::compare_cost_nrv_from_json,
    );
    map.insert(
        "acquisition",
        (|p| via_seam(bridge::assurance::acquisition_json, p)) as Calculator,
    );
    map.insert(
        "capex-budget",
        (|p| via_seam(bridge::assurance::capex_budget_json, p)) as Calculator,
    );
    map.insert(
        "control-evaluation",
        (|p| via_seam(bridge::assurance::control_evaluation_json, p)) as Calculator,
    );
    map.insert(
        "controls",
        (|p| via_seam(bridge::assurance::controls_json, p)) as Calculator,
    );
    map.insert(
        "credit",
        (|p| via_seam(bridge::assurance::credit_json, p)) as Calculator,
    );
    map.insert(
        "credit-risk",
        (|p| via_seam(bridge::assurance::credit_risk_json, p)) as Calculator,
    );
    map.insert(
        "distress",
        (|p| via_seam(bridge::assurance::distress_json, p)) as Calculator,
    );
    map.insert(
        "fraud-risk",
        (|p| via_seam(bridge::assurance::fraud_risk_json, p)) as Calculator,
    );
    map.insert(
        "fx-convert",
        (|p| via_seam_today(bridge::assurance::fx_convert_json, p)) as Calculator,
    );
    map.insert(
        "fx-hedge",
        (|p| via_seam(bridge::assurance::fx_hedge_json, p)) as Calculator,
    );
    map.insert(
        "goodwill-impairment",
        (|p| via_seam(bridge::assurance::goodwill_impairment_json, p)) as Calculator,
    );
    map.insert(
        "grant",
        (|p| via_seam(bridge::assurance::grant_json, p)) as Calculator,
    );
    map.insert(
        "group-tax",
        (|p| via_seam(bridge::assurance::group_tax_json, p)) as Calculator,
    );
    map.insert(
        "inventory",
        (|p| via_seam(bridge::assurance::inventory_json, p)) as Calculator,
    );
    map.insert(
        "nrv",
        (|p| via_seam(bridge::assurance::nrv_json, p)) as Calculator,
    );
    map.insert(
        "nrv-compare",
        (|p| via_seam(bridge::assurance::nrv_compare_json, p)) as Calculator,
    );
    map.insert(
        "options-price",
        (|p| via_seam(bridge::assurance::options_price_json, p)) as Calculator,
    );
    map.insert(
        "partner-buyout",
        (|p| via_seam(bridge::assurance::partner_buyout_json, p)) as Calculator,
    );
    map.insert(
        "partnership-allocation",
        (|p| via_seam(bridge::assurance::partnership_allocation_json, p)) as Calculator,
    );
    map.insert(
        "partnership-goodwill",
        (|p| via_seam(bridge::assurance::partnership_goodwill_json, p)) as Calculator,
    );
    map.insert(
        "pd-historical",
        (|p| via_seam(bridge::assurance::pd_historical_json, p)) as Calculator,
    );
    map.insert(
        "pension",
        (|p| via_seam(bridge::assurance::pension_json, p)) as Calculator,
    );
    map.insert(
        "profit",
        (|p| via_seam(bridge::assurance::profit_json, p)) as Calculator,
    );
    map.insert("profit-calculate", (|p| profit_calculate(p)) as Calculator);
    map.insert(
        "profit-from-cash-flow",
        (|p| profit_from_cash_flow(p)) as Calculator,
    );
    map.insert(
        "profit-average-profit",
        (|p| profit_average_profit(p)) as Calculator,
    );
    map.insert(
        "profit-average-accounting-profit",
        (|p| profit_average_accounting_profit(p)) as Calculator,
    );
    map.insert(
        "profit-net-profit",
        (|p| profit_net_profit(p)) as Calculator,
    );
    map.insert(
        "profit-profit-compare",
        (|p| profit_compare(p)) as Calculator,
    );
    map.insert(
        "regulatory-ratio",
        (|p| via_seam(bridge::assurance::regulatory_ratio_json, p)) as Calculator,
    );
    map.insert(
        "residual",
        (|p| via_seam(bridge::assurance::residual_json, p)) as Calculator,
    );
    map.insert(
        "scorecard",
        (|p| via_seam(bridge::assurance::scorecard_json, p)) as Calculator,
    );
    map.insert(
        "scrap",
        (|p| via_seam(bridge::assurance::scrap_json, p)) as Calculator,
    );
    map.insert(
        "tax-risk",
        (|p| via_seam(bridge::assurance::tax_risk_json, p)) as Calculator,
    );
    map.insert(
        "var",
        (|p| via_seam(bridge::assurance::var_json, p)) as Calculator,
    );
    map.insert(
        "vat-return",
        (|p| via_seam(bridge::assurance::vat_return_json, p)) as Calculator,
    );
    map.insert(
        "xbrl",
        (|p| via_seam_today(bridge::assurance::xbrl_json, p)) as Calculator,
    );
    map.insert("margin-of-safety", (|p| margin_safety(p)) as Calculator);
    map.insert("arr-decision", (|p| arr_decision(p)) as Calculator);
    map.insert(
        "discounted-payback",
        (|p| discounted_payback(p)) as Calculator,
    );
    map.insert(
        "treasury-analytics",
        (|p| treasury_analytics(p)) as Calculator,
    );
    map.insert(
        "accounting-rate-return-calculate",
        (|p| arr_calculate(p)) as Calculator,
    );
    map.insert(
        "accounting-rate-return-from-annual-profits",
        (|p| arr_from_annual_profits(p)) as Calculator,
    );
    map.insert(
        "accounting-rate-return-average-profit",
        (|p| arr_average_profit(p)) as Calculator,
    );
    map.insert(
        "accounting-rate-return-average-capital",
        (|p| arr_average_capital(p)) as Calculator,
    );
    map.insert(
        "accounting-rate-return-compare",
        (|p| arr_compare(p)) as Calculator,
    );
    map.insert(
        "accounting-rate-return-decision",
        (|p| arr_decision(p)) as Calculator,
    );
    map.insert(
        "discount-factor-calculate",
        (|p| df_calculate(p)) as Calculator,
    );
    map.insert(
        "discount-factor-calculate-percent",
        (|p| df_calculate_percent(p)) as Calculator,
    );
    map.insert(
        "discount-factor-annuity-factor",
        (|p| df_annuity(p)) as Calculator,
    );
    map.insert(
        "discount-factor-annuity-factor-percent",
        (|p| df_annuity_percent(p)) as Calculator,
    );
    map.insert("discount-factor-table", (|p| df_table(p)) as Calculator);
    map.insert(
        "discount-factor-table-range",
        (|p| df_table_range(p)) as Calculator,
    );
    map.insert(
        "marginal-costing-contribution",
        (|p| marginal_contribution(p)) as Calculator,
    );
    map.insert(
        "marginal-costing-profit-forecast",
        (|p| marginal_forecast(p)) as Calculator,
    );
    map.insert(
        "marginal-costing-cost-item",
        (|p| marginal_cost_item_adapter(p)) as Calculator,
    );
    map.insert(
        "marginal-costing-income-statement",
        (|p| marginal_income_statement_adapter(p)) as Calculator,
    );
    map.insert(
        "segment-reporting-report",
        ratios_statements::segment_reporting::report_from_json,
    );
    map.insert(
        "interim-reporting-year-to-date",
        ratios_statements::interim_reporting::year_to_date_from_json,
    );
    map.insert(
        "interim-reporting-effective-tax-rate",
        ratios_statements::interim_reporting::effective_tax_rate_from_json,
    );
    map.insert(
        "currency-hedging-analyze",
        (|p| currency_hedging_analyze(p)) as Calculator,
    );
    map.insert(
        "commodity-hedging-analyze",
        (|p| commodity_hedging_analyze(p)) as Calculator,
    );
    map.insert(
        "futures-hedging-analyze",
        (|p| futures_hedging_analyze(p)) as Calculator,
    );
    map.insert(
        "interest-rate-hedging-analyze",
        (|p| interest_rate_hedging_analyze(p)) as Calculator,
    );
    map.insert(
        "cross-currency-swap-value",
        (|p| cross_currency_swap_value(p)) as Calculator,
    );
    map.insert(
        "multi-currency-convert",
        (|p| multi_currency_convert(p)) as Calculator,
    );
    map.insert(
        "multi-currency-hedge",
        (|p| multi_currency_hedge(p)) as Calculator,
    );
    map.insert(
        "multi-currency-transaction",
        (|p| multi_currency_transaction(p)) as Calculator,
    );
    map.insert(
        "foreign-currency-translation-translate",
        (|p| foreign_currency_translate(p)) as Calculator,
    );
    map.insert(
        "provisions-contingencies-assess",
        ifrs::provisions_assess_from_json,
    );
    map.insert(
        "acquisition-financing-structure",
        capital_valuation::acquisition_financing_from_json,
    );
    map.insert(
        "project-accounting-costs",
        reporting::project_costs_from_json,
    );
    map.insert(
        "project-accounting-budget",
        reporting::project_budget_from_json,
    );
    map.insert("project-accounting-wip", reporting::project_wip_from_json);
    map.insert(
        "business-combination-acquire",
        group_accounting::business_combination_from_json,
    );
    map.insert(
        "audit-evidence-evaluate",
        assurance::audit_evidence_from_json,
    );
    map.insert("going-concern-assess", assurance::going_concern_from_json);
    map.insert(
        "corporate-governance-assess",
        assurance::corporate_governance_from_json,
    );
    map.insert("due-diligence-assess", assurance::due_diligence_from_json);
    map.insert("esg-reporting-report", reporting::esg_report_from_json);
    map.insert(
        "xbrl-reporting-generate",
        reporting::xbrl_generate_from_json,
    );
    map.insert(
        "ratio-analysis-ratios",
        ratios_statements::ratio_analysis_ratios_from_json,
    );
    map.insert(
        "ratio-analysis-peer-comparison",
        ratios_statements::ratio_analysis_peer_comparison_from_json,
    );
    map.insert(
        "ratio-analysis-trend",
        ratios_statements::ratio_analysis_trend_from_json,
    );
    map.insert("tax-calculate", tax::tax_calculate_from_json);
    map.insert("tax-provision", tax::tax_provision_from_json);
    map.insert("tax-transfer-pricing", tax::tax_transfer_pricing_from_json);
    map.insert("tax-compliance", tax::tax_compliance_from_json);
    map.insert(
        "trial-balance-generate",
        ratios_statements::trial_balance_generate_from_json,
    );
    map.insert(
        "trial-balance-from-ledgers",
        ratios_statements::trial_balance_from_ledgers_from_json,
    );
    map.insert(
        "trial-balance-validate",
        ratios_statements::trial_balance_validate_from_json,
    );
    map.insert(
        "trial-balance-classification-summary",
        ratios_statements::trial_balance_classification_summary_from_json,
    );
    map.insert(
        "trial-balance-adjusted",
        ratios_statements::trial_balance_adjusted_from_json,
    );
    map.insert(
        "mva-service-calculate",
        capital_valuation::mva_service_from_json,
    );
    map.insert(
        "sustainable-growth-calculate",
        capital_valuation::sustainable_growth_service_from_json,
    );
    map.insert(
        "asset-allocation-allocate",
        capital_valuation::asset_allocation_service_from_json,
    );
    map.insert(
        "project-appraisal-appraise",
        capital_valuation::project_appraisal_service_from_json,
    );
    map.insert(
        "portfolio-optimization-optimize",
        capital_valuation::portfolio_optimization_service_from_json,
    );
    map.insert(
        "capital-allocation-allocate",
        capital_valuation::capital_allocation_service_from_json,
    );
    map.insert(
        "investment-appraisal-appraise",
        capital_valuation::investment_appraisal_service_from_json,
    );
    map.insert("cash-budget-prepare", budgeting::cash::prepare_from_json);
    map.insert(
        "zero-based-budget-prepare",
        budgeting::zero_based::prepare_from_json,
    );
    map.insert(
        "master-budget-prepare",
        budgeting::master::prepare_from_json,
    );
    map.insert(
        "flexible-budget-calculate",
        budgeting::flexible::calculate_from_json,
    );
    map.insert(
        "flexible-budget-multi-level",
        budgeting::flexible::multi_level_from_json,
    );
    map.insert(
        "flexible-budget-variance-analysis",
        budgeting::flexible::variance_from_json,
    );
    map
}

#[derive(Clone)]
pub struct AppState {
    pub calculators: Arc<BTreeMap<&'static str, Calculator>>,
}

/// A calculator served at the address the Python service published for it.
///
/// The calculator registry is the one place these sums live, so an address does not get its
/// own copy of the arithmetic: it looks the calculator up by the same name `/calc/{name}`
/// uses. Arguments come from the body and, as with the cost-accounting endpoints, from the
/// query string, because these services took theirs as query parameters.
fn published(name: &'static str) -> axum::routing::MethodRouter<AppState> {
    axum::routing::post(
        move |State(state): State<AppState>,
              RawQuery(query): RawQuery,
              body: Option<Json<Value>>| async move {
            let payload = merge_request(query.as_deref(), body.map(|Json(value)| value));
            match state.calculators.get(name) {
                Some(calculator) => match calculator(&payload) {
                    Ok(answer) => (StatusCode::OK, Json(answer)).into_response(),
                    Err(message) => (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": message })),
                    )
                        .into_response(),
                },
                None => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("{name} is not registered") })),
                )
                    .into_response(),
            }
        },
    )
}

/// Build the router and, when a broker was configured, start taking events off it.
///
/// The relay is started here rather than in `main` so that anything serving this router - the binary, a test harness, the
/// on-device runtime - gets the same behaviour. With no broker asked for, nothing is started and the statistics say so.
pub fn router() -> Router {
    let graph = records::from_env();
    start_broker(graph.clone());
    routed(graph)
}

/// Start this pod's consumer: every event the bus publishes reaches this process's sockets from here.
fn start_broker(graph: Option<records::GraphHandle>) {
    let Some(url) = events::broker::url_from_env() else {
        return;
    };
    let Some(graph) = graph else {
        // with no store there is nothing to record deliveries against, so a broker would be carrying into thin air
        return;
    };
    events::broker::start_consuming(url, move |event| {
        let scope = store::BookScope::new(
            event
                .get("payload")
                .and_then(|payload| payload.get("user_id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("system"),
            event
                .get("payload")
                .and_then(|payload| payload.get("book_id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        );
        // the same carry the delivery door performs, so a pod reached by the broker answers exactly as one reached over
        // HTTP: one channel, one message shape, one copy per event even when both transports deliver it
        let _ = events::realtime::ingest(
            graph.as_ref(),
            &scope,
            events::sockets::hub(),
            &event,
            events::realtime::DEDUPE_WINDOW,
        );
    });
}

fn routed(graph: Option<records::GraphHandle>) -> Router {
    let state = AppState {
        calculators: Arc::new(calculators()),
    };
    let calculations = Router::new()
        .route("/health", get(health))
        .route("/calculators", get(list))
        .route("/calc/:name", post(run))
        // biological assets and investment property publish these beside their prefix, so a client
        // calls the service's own address and the engine answers from the same calculator
        .route(
            "/biological-assets/measure",
            published("biological-assets-measure"),
        )
        .route(
            "/biological-assets/fair-value-level1",
            published("biological-assets-fair-value-level1"),
        )
        .route(
            "/biological-assets/fair-value-level2",
            published("biological-assets-fair-value-level2"),
        )
        .route(
            "/biological-assets/fair-value-level3",
            published("biological-assets-fair-value-level3"),
        )
        .route(
            "/biological-assets/consumable-bearer-distinction",
            published("biological-assets-consumable-bearer-distinction"),
        )
        .route(
            "/biological-assets/disclosure-template",
            published("biological-assets-disclosure-template"),
        )
        .route(
            "/investment-property/fair-value-model",
            published("investment-property-fair-value-model"),
        )
        .route(
            "/investment-property/cost-model",
            published("investment-property-cost-model"),
        )
        .route(
            "/investment-property/transfer",
            published("investment-property-transfer"),
        )
        .route(
            "/investment-property/yields",
            published("investment-property-yields"),
        )
        .route(
            "/investment-property/fair-value-disclosure",
            published("investment-property-fair-value-disclosure"),
        )
        .with_state(state);
    // the endpoints that own records need a caller, so they are a separate mount
    match graph {
        Some(graph) => calculations.merge(records::router(graph)),
        None => calculations,
    }
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "engine": "rust",
        "calculators": state.calculators.len(),
    }))
}

/// The request a calculator is given, from the body and the query string.
///
/// Most calculators take a JSON body, but the cost-accounting endpoints took their
/// arguments as query parameters (`POST /variance-analysis?standard_cost=900`), so a
/// client of those has no body to send. Rather than make the route a different
/// contract from the service it replaces, both are accepted: the body is the
/// request, and any query parameters fill in fields it does not mention.
fn merge_request(query: Option<&str>, body: Option<Value>) -> Value {
    let mut merged = match body {
        Some(Value::Object(map)) => map,
        Some(other) => return other,
        None => serde_json::Map::new(),
    };
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or("").as_bytes()) {
        if merged.contains_key(key.as_ref()) {
            continue;
        }
        // query values arrive as text; give the calculator the number the service's
        // own type annotation would have given it, and plain text otherwise
        let filled = match value.parse::<f64>() {
            Ok(number) if value.trim() == number.to_string() => json!(number),
            _ => match value.as_ref() {
                "true" => json!(true),
                "false" => json!(false),
                _ => json!(value.as_ref()),
            },
        };
        merged.insert(key.into_owned(), filled);
    }
    Value::Object(merged)
}

async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let names: Vec<&str> = state.calculators.keys().copied().collect();
    Json(json!({ "calculators": names }))
}

async fn run(
    State(state): State<AppState>,
    Path(name): Path<String>,
    RawQuery(query): RawQuery,
    body: Option<Json<Value>>,
) -> Response {
    let payload = merge_request(query.as_deref(), body.map(|Json(value)| value));
    match state.calculators.get(name.as_str()) {
        Some(calculator) => match calculator(&payload) {
            Ok(answer) => (StatusCode::OK, Json(answer)).into_response(),
            Err(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": message })),
            )
                .into_response(),
        },
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": format!("no calculator named {name}"),
                "calculators": state.calculators.keys().copied().collect::<Vec<_>>(),
            })),
        )
            .into_response(),
    }
}

/// Some calculations only ever had a Python entry point: they take a JSON string and
/// return one. These adapters put the same maths behind the HTTP calculator registry,
/// so the two front ends cannot drift apart.
fn via_seam(
    calculate: fn(&str) -> Result<String, String>,
    payload: &Value,
) -> Result<Value, String> {
    let rendered = calculate(&payload.to_string())?;
    serde_json::from_str(&rendered).map_err(|error| error.to_string())
}

fn via_seam_today(
    calculate: fn(&str, &str) -> Result<String, String>,
    payload: &Value,
) -> Result<Value, String> {
    let rendered = calculate(&payload.to_string(), &today())?;
    serde_json::from_str(&rendered).map_err(|error| error.to_string())
}

/// Today as an ISO date, for the seams that date their own defaults.
fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
/// These four calculations take typed arguments rather than a JSON payload, so they had
/// no way to answer an HTTP request. Rather than reimplement them, these adapters read
/// the fields the Python service always sent and call the same maths.
fn number(payload: &Value, key: &str) -> Result<f64, String> {
    match payload.get(key) {
        Some(Value::Number(value)) => value
            .as_f64()
            .ok_or_else(|| format!("{key} is not a number")),
        Some(Value::String(text)) => text
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{key} is not a number")),
        Some(other) => Err(format!("{key} is {other}, expected a number")),
        None => Err(format!("{key} is required")),
    }
}

fn number_list(payload: &Value, key: &str) -> Result<Vec<f64>, String> {
    let items = payload
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} must be a list of numbers"))?;
    items
        .iter()
        .map(|item| match item {
            Value::Number(value) => value
                .as_f64()
                .ok_or_else(|| format!("{key} holds a non-number")),
            Value::String(text) => text
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("{key} holds a non-number")),
            _ => Err(format!("{key} holds a non-number")),
        })
        .collect()
}

fn to_value<T: serde::Serialize>(output: &T) -> Result<Value, String> {
    serde_json::to_value(output).map_err(|error| error.to_string())
}

fn margin_safety(payload: &Value) -> Result<Value, String> {
    let output = cost_accounting::margin_safety::margin_of_safety(
        number(payload, "current_output")?,
        number(payload, "current_revenue")?,
        number(payload, "break_even_output")?,
        number(payload, "break_even_revenue")?,
    );
    to_value(&output)
}

fn arr_decision(payload: &Value) -> Result<Value, String> {
    Ok(capital_valuation::capital_returns::arr_decision(
        number(payload, "arr")?,
        number(payload, "target_return")?,
    ))
}

fn discounted_payback(payload: &Value) -> Result<Value, String> {
    let flows = number_list(payload, "cash_flows")?;
    let output = capital_valuation::capital_returns::discounted_payback(
        number(payload, "initial_investment")?,
        &flows,
        number(payload, "discount_rate")?,
    );
    to_value(&output)
}

fn treasury_analytics(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::analytics::TreasuryAnalyticsRequest =
        serde_json::from_value(payload.clone())
            .map_err(|error| format!("invalid treasury analytics request: {error}"))?;
    to_value(&treasury_fx::analytics::analyze_treasury(&request))
}

#[cfg(test)]
mod typed_adapters {
    use super::*;
    use serde_json::json;

    fn calculate(name: &str, payload: Value) -> Value {
        let registry = calculators();
        let entry = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        entry(&payload).unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    #[test]
    fn margin_of_safety_takes_the_service_field_names() {
        let out = calculate(
            "margin-of-safety",
            json!({
                "current_output": 1000.0,
                "current_revenue": 50000.0,
                "break_even_output": 800.0,
                "break_even_revenue": 40000.0
            }),
        );
        assert!(out.is_object());
        assert_eq!(out["margin_of_safety_units"], json!(200.0));
    }

    #[test]
    fn payroll_accounting_processes_a_payslip_run_exactly() {
        let out = calculate(
            "payroll-accounting-process",
            json!({
                "company_id": "c-1",
                "period": "2026-11",
                "employees": [
                    {"employee_id": "e-1", "employee_name": "Sam", "gross_salary": "10000.00",
                     "medical_aid": "300.00", "other_deductions": "200.00"},
                    {"employee_id": "e-2", "employee_name": "Ada", "gross_salary": "6000.00",
                     "paye_rate": 0.2, "nassa_rate": 0.0, "pension_rate": 0.0}
                ]
            }),
        );
        assert_eq!(out["employee_count"], json!(2));
        assert_eq!(out["total_gross"], json!("16000.00"));
        assert_eq!(out["total_paye"], json!("3700.00"));
        // a rate of zero is a rate, not a missing field
        assert_eq!(out["total_nassa"], json!("300.00"));
        assert_eq!(out["total_cost_to_company"], json!("17600.00"));
        assert_eq!(out["journal_entries"].as_array().map(Vec::len), Some(8));
    }

    #[test]
    fn payroll_accounting_refuses_an_amount_it_cannot_read() {
        let registry = calculators();
        let entry = registry
            .get("payroll-accounting-process")
            .expect("registered");
        let error = entry(&json!({
            "company_id": "c-1",
            "period": "2026-11",
            "employees": [{"gross_salary": "not a number"}]
        }))
        .expect_err("an unreadable amount is refused");
        assert!(error.contains("invalid request"), "{error}");
    }

    #[test]
    fn business_valuation_answers_all_five_endpoints() {
        let company = json!({
            "company_id": "c-1", "company_name": "Vimbai", "fiscal_year_end": "2026-12-31",
            "revenue": "5000000", "ebitda": "1000000", "ebit": "800000", "net_income": "500000",
            "total_assets": "9000000", "total_equity": "4000000", "total_debt": "2000000",
            "cash": "500000", "shares_outstanding": "1000000", "share_price": "12.50",
            "dividend_per_share": "0.80"
        });
        let out = calculate(
            "business-valuation",
            json!({
                "company": company,
                "dcf_inputs": {
                    "projection_years": 5, "base_revenue": "5000000", "revenue_cagr": 0.1,
                    "ebitda_margin": 0.2, "depreciation_rate": 0.03, "capex_rate": 0.04,
                    "tax_rate": 0.25, "discount_rate": 0.1, "terminal_growth_rate": 0.03,
                    "net_debt": "1500000"
                }
            }),
        );
        assert!(out["dcf_valuation"]["equity_value"].is_string());
        assert!(out["dividend_discount_valuation"]["value_per_share"].is_string());
        // the surface stamps the date a stored valuation would have supplied itself
        assert!(out["valuation_date"]
            .as_str()
            .map(|date| date.len() == 10)
            .unwrap_or(false));

        let wacc = calculate(
            "business-valuation-wacc",
            json!({"market_cap": "6000000", "total_debt": "2000000",
                   "cost_of_equity": 0.12, "cost_of_debt": 0.08, "tax_rate": 0.25}),
        );
        assert_eq!(wacc["equity_weight"], json!(0.75));

        let capm = calculate(
            "business-valuation-capm",
            json!({"risk_free_rate": 0.05, "market_return": 0.12, "beta": 1.2}),
        );
        assert!(capm["cost_of_equity"].is_number());

        let precedent = calculate(
            "business-valuation-precedent-transaction",
            json!({"transaction_price": "45000000", "target_ebitda": "5000000",
                   "target_revenue": "30000000", "premium_paid": 0.25}),
        );
        assert_eq!(precedent["implied_ev_ebitda"], json!(9.0));

        let book = calculate(
            "business-valuation-book-value",
            json!({"total_assets": "9000000", "intangible_assets": "1000000",
                   "total_liabilities": "5000000", "preferred_equity": "500000",
                   "minority_interests": "250000"}),
        );
        assert_eq!(book["tangible_book_value"], json!("3000000"));
    }

    #[test]
    fn financial_statements_prepare_answers_with_nested_statements() {
        let out = calculate(
            "financial-statements-prepare",
            json!({
                "company_id": "c-1",
                "period": "2026",
                "balance_sheet": {
                    "current_assets": "400000", "non_current_assets": "600000",
                    "current_liabilities": "200000", "non_current_liabilities": "300000",
                    "inventory": "100000"
                },
                "income_statement": {
                    "revenue": "1000000", "net_income": "150000",
                    "ebitda": "260000", "depreciation": "60000", "cogs": "600000"
                }
            }),
        );
        assert_eq!(
            out["statements"]["income_statement"]["ebit"],
            json!("200000")
        );
        assert_eq!(out["key_ratios"]["gross_margin"], json!(40.0));
        assert_eq!(out["financial_health_score"], json!(93.5));
        assert!(out["recommendations"].as_array().map(Vec::len) == Some(0));

        let huge = calculate(
            "financial-statements-prepare",
            json!({
                "company_id": "c-1",
                "period": "2026",
                "balance_sheet": {
                    "current_assets": "123456789012345678901.55",
                    "current_liabilities": "123456789012345678901.55"
                }
            }),
        );
        assert_eq!(
            huge["statements"]["balance_sheet"]["total_assets"],
            json!("123456789012345678901.55")
        );
        assert_eq!(huge["key_ratios"]["current_ratio"], json!(1.0));
    }

    #[test]
    fn ifrs_15_answers_all_six_endpoints() {
        let recognized = calculate(
            "ifrs-15-recognize",
            json!({
                "company_id": "c-1",
                "reporting_date": "2026-12-31",
                "contracts": [{
                    "contract_id": "K1", "customer_id": "cust-1",
                    "performance_obligation_ids": ["po1", "po2"],
                    "transaction_price": "100000"
                }],
                "performance_obligations": [
                    {"obligation_id": "po1", "description": "Licence",
                     "obligation_type": "point_in_time", "standalone_selling_price": "60000"},
                    {"obligation_id": "po2", "description": "Installation",
                     "obligation_type": "over_time", "standalone_selling_price": "40000"}
                ],
                "actual_progress": [["po1", 1.0], ["po2", 0.4]]
            }),
        );
        assert_eq!(recognized["total_revenue_recognized"], json!("76000.00"));
        assert_eq!(
            recognized["step3_allocations"][0]["allocations"][1][1],
            json!("40000.00")
        );

        let variable = calculate(
            "ifrs-15-variable-consideration",
            json!({"contract_id": "K1", "variable_type": "royalty",
                   "expected_value": "50000", "most_likely_amount": "10",
                   "constraint_applied": 1.5}),
        );
        assert_eq!(variable["constrained_estimate"], json!("50000"));

        let ssp = calculate(
            "ifrs-15-standalone-selling-price",
            json!({"obligation_id": "po1", "observable_price": "55000",
                   "adjusted_market_assessment": "60000", "expected_cost_plus_margin": "48000",
                   "allocation_percentage": 60.0, "transaction_price": "100000"}),
        );
        assert_eq!(ssp["allocated_transaction_price"], json!("60000.00"));

        let progress = calculate(
            "ifrs-15-progress-measurement",
            json!({"obligation_id": "units_delivered", "method": "output_methods",
                   "units_delivered": 30.0, "total_units": 120.0, "costs_incurred": "0",
                   "total_estimated_costs": "0", "resources_consumed": 0.0,
                   "total_resources": 0.0}),
        );
        assert_eq!(progress["progress_percentage"], json!(25.0));

        let option = calculate(
            "ifrs-15-customer-options",
            json!({"option_type": "future_purchase", "has_material_right": true,
                   "standalone_selling_price": "1000", "discount_rate": 0.2,
                   "expected_exercise": 60.0}),
        );
        assert_eq!(option["present_value"], json!("800.00"));

        let modification = calculate(
            "ifrs-15-modification",
            json!({"original_contract_id": "K1", "modification_type": "price_change",
                   "original_price": "100000", "new_price": "120000",
                   "original_progress": 0.4, "modification_date": "2026-05-01"}),
        );
        assert_eq!(modification["additional_revenue"], json!("12000.00"));
    }

    #[test]
    fn ifrs_2_answers_all_five_endpoints() {
        let out = calculate(
            "ifrs-2-calculate",
            json!({
                "company_id": "c-1",
                "reporting_date": "2026-12-31",
                "equity_settled_grants": [{
                    "option_id": "E1", "grant_date": "2024-01-01",
                    "number_of_options": 10000, "exercise_price": "5.00",
                    "fair_value_per_option": "2.00", "vesting_period_years": 4,
                    "vesting_conditions": [{
                        "condition_id": "vc", "condition_type": "service",
                        "description": "service", "target_value": 1.0,
                        "probability_assessment": 0.9, "probability_weight": 0.9
                    }]
                }],
                "share_price_at_reporting": "10.00"
            }),
        );
        assert_eq!(out["equity_settled_expense"], json!("3825.00"));
        assert_eq!(
            out["equity_settled_vesting"][0]["options_expected_to_vest"],
            json!(7650)
        );

        let fair = calculate(
            "ifrs-2-fair-value-equity-instruments",
            json!({"option_type": "lookback", "spot_price": "100", "exercise_price": "90",
                   "time_to_expiry": 2.0, "risk_free_rate": 0.05, "volatility": 0.25,
                   "dividend_yield": 0.0}),
        );
        assert_eq!(fair["adjustment_applied"], json!(1.2));
        assert!(fair["calculated_fair_value"].is_string());

        let perf = calculate(
            "ifrs-2-performance-condition",
            json!({"condition_type": "EPS_growth", "target_metric": 100.0,
                   "current_metric": 60.0, "measurement_period": 1.0}),
        );
        assert_eq!(perf["expected_achievement_probability"], json!(0.64));

        let modification = calculate(
            "ifrs-2-modification",
            json!({"original_grant_date": "2024-01-01", "original_fair_value": "5.00",
                   "original_options": 10000, "modification_date": "2026-01-01",
                   "new_fair_value": "6.00", "new_options": 10000, "new_vesting_period": 2,
                   "years_elapsed": 5, "cumulative_expense_original": "1.00"}),
        );
        assert_eq!(modification["catch_up_expense"], json!("14.00"));

        let group = calculate(
            "ifrs-2-group-transaction",
            json!({"parent_equity_settled": "0", "parent_cash_settled": "100000",
                   "non_controlling_interests": "50000", "transaction_type": "cash_for_equity"}),
        );
        assert_eq!(group["nci_impact"], json!("5000.00"));
        assert_eq!(group["financial_statement_impact"], json!("liability"));
    }

    #[test]
    fn ifrs_9_answers_all_six_endpoints() {
        let out = calculate(
            "ifrs-9-classify",
            json!({
                "company_id": "c-1",
                "reporting_date": "2026-12-31",
                "financial_assets": [{
                    "asset_id": "A1", "asset_name": "Loan", "asset_type": "debt",
                    "category": "amortized_cost", "carrying_amount": "1000000",
                    "effective_interest_rate": 0.05, "expected_credit_loss_stage": 1
                }],
                "base_pdg": 0.01, "base_lgd": 0.45
            }),
        );
        assert_eq!(out["total_ecl"], json!("4500.00"));
        assert_eq!(out["total_amortized_cost"], json!("995500.00"));

        let sppi = calculate(
            "ifrs-9-sppi-test",
            json!({"instrument_type": "structured", "principal_amount": "1000000",
                   "interest_type": "contingent", "contingency_feature": "credit_sensitive",
                   "notional_linked": true}),
        );
        assert_eq!(sppi["sppi_test_passed"], json!(false));
        assert_eq!(sppi["failed_criteria"].as_array().map(Vec::len), Some(3));

        let model = calculate(
            "ifrs-9-business-model",
            json!({"holding_purpose": "collect_and_sell", "sales_frequency": "occasional",
                   "sales_proceeds": "200000", "total_income": "1000000"}),
        );
        assert_eq!(model["measurement_basis"], json!("FVOCI"));
        assert_eq!(model["sales_proceeds_ratio"], json!(0.2));

        let general = calculate(
            "ifrs-9-ecl-general",
            json!({"exposure_at_default": "1000000", "probability_of_default": 0.02,
                   "loss_given_default": 0.45, "discount_rate": 0.05,
                   "time_to_default_years": 3.0}),
        );
        assert_eq!(general["undiscounted_ecl"], json!("9000.00"));

        let simplified = calculate(
            "ifrs-9-ecl-simplified",
            json!({"trade_receivables_ageing": [["current", "500000"], ["90_days", "100000"]],
                   "historical_loss_rates": [["current", 0.005]],
                   "forward_looking_adjustment": 0.2}),
        );
        assert_eq!(simplified["total_ecl"], json!("4200.00"));
        assert_eq!(simplified["age_breakdown"][1]["loss_rate"], json!(0.01));

        let hedge = calculate(
            "ifrs-9-hedge-effectiveness",
            json!({"hedge_type": "cash_flow", "hypothetical_derivative": "1000000",
                   "actual_hedge_instrument": "900000", "regression_correlation": 0.85}),
        );
        assert_eq!(hedge["hedge_ratio"], json!(0.9));
        assert_eq!(hedge["rebalancing_required"], json!(true));
    }

    #[test]
    fn corporate_event_capabilities_are_reachable() {
        let deal = calculate(
            "deal-structuring-structure",
            json!({"deal_id": "D1", "target_id": "T1", "total_value": "1000.00",
                   "seller_preference": "stock", "earnout_required": true,
                   "escrow_percentage": 0.05, "deal_date": "2026-11-03"}),
        );
        assert_eq!(
            deal["structure_options"][1]["upfront_payment"],
            json!("800.00")
        );
        assert_eq!(
            deal["recommended_structure"]["structure_type"],
            json!("Share Exchange")
        );
        assert_eq!(deal["deal_date"], json!("2026-11-03"));

        let post = calculate(
            "post-merger-track",
            json!({"deal_id": "D1", "synergies_achieved": "75", "synergies_target": "100",
                   "integration_costs_actual": "55", "integration_costs_budget": "40",
                   "days_since_close": 45, "status_date": "2026-11-03",
                   "milestones": [{"milestone_id": "M1", "status": "Completed",
                                   "completion_percentage": 100.0}]}),
        );
        assert_eq!(post["synergy_performance"]["variance"], json!("-25.00"));
        assert_eq!(post["overall_status"], json!("Red"));

        let synergy = calculate(
            "synergy-analysis-analyze",
            json!({"deal_id": "D1", "integration_costs": "150",
                   "cost_of_capital": 0.09,
                   "synergies": [{"category": "cost", "annual_value": "200",
                                  "probability": 0.8, "timing_months": 6.0}]}),
        );
        assert_eq!(
            synergy["synergy_summary"]["cost_synergies"],
            json!("160.00")
        );
        assert_eq!(synergy["pv_integration_costs"], json!("165.00"));
    }

    #[test]
    fn merger_valuation_is_reachable_and_exact() {
        let out = calculate(
            "merger-valuation-value",
            json!({
                "acquirer_id": "ACQ", "target_id": "TGT",
                "acquirer_equity": "1000", "target_equity": "400",
                "acquirer_ebitda": "150", "target_ebitda": "50",
                "acquirer_revenue": "900", "target_revenue": "300",
                "synergies_ebitda": "20", "acquirer_pe": 18.0, "target_pe": 12.0,
                "deal_premium": 0.25, "transaction_costs": "15", "integration_costs": "30"
            }),
        );
        assert_eq!(out["recommended_offer_price"], json!("500.00"));
        assert_eq!(out["total_deal_value"], json!("515.00"));
        assert_eq!(out["synergy_value"], json!("200.00"));
        assert_eq!(out["accretive_dilutive"], json!("Accretive"));

        // a figure that is not money is refused rather than read as zero
        let registry = calculators();
        let entry = registry.get("merger-valuation-value").expect("registered");
        let error = entry(&json!({"acquirer_id": "ACQ", "target_equity": "not money"}))
            .expect_err("refused");
        assert!(error.contains("invalid request"), "{error}");
    }

    #[test]
    fn budgeting_and_suspense_capabilities_are_reachable() {
        let scenarios = calculate(
            "scenario-budget-analyze",
            json!({"company_id": "acme", "base_revenue": "1000000",
                   "base_expenses": "800000", "worst_case_multiplier": 0.75,
                   "best_case_multiplier": 1.25}),
        );
        assert_eq!(scenarios["worst_case"]["profit"], json!("30000.00"));
        assert_eq!(
            scenarios["risk_assessment"],
            json!("Medium risk - wide range between scenarios")
        );

        let pnl = calculate(
            "profit-loss-prepare",
            json!({"company_id": "c1", "revenue": "900000", "other_income": "50000",
                   "cost_of_goods_sold": "540000", "operating_expenses": "120000",
                   "depreciation": "30000", "interest_expense": "20000",
                   "other_expenses": "5000", "tax_rate": 0.25}),
        );
        assert_eq!(pnl["net_income"], json!("138750.00"));
        assert_eq!(pnl["ebit"], json!("210000.00"));

        let compared = calculate(
            "profit-loss-compare",
            json!({"company_id": "c1", "revenue": "900000",
                   "cost_of_goods_sold": "540000", "operating_expenses": "120000",
                   "depreciation": "30000", "interest_expense": "20000",
                   "other_expenses": "5000", "tax_rate": 0.25,
                   "previous_revenue": "800000", "previous_net_income": "100000"}),
        );
        assert_eq!(compared["revenue_change_pct"], json!(12.5));

        let suspense = calculate(
            "suspense-error-analyze",
            json!({"company_id": "c1", "period": "2026-11", "suspense_balance": "1000",
                   "errors": [{"error_type": "omission", "description": "d",
                               "amount": "150", "account_affected": "Sales"}]}),
        );
        assert_eq!(suspense["corrected_suspense_balance"], json!("850.00"));

        let kpis = calculate(
            "kpi-tracking-track",
            json!({"company_id": "C1", "reporting_period": "2026-10", "kpis": [
                {"kpi_id": "K1", "kpi_name": "Margin", "current_value": "32",
                 "target": "30", "previous_value": "28", "unit": "%"}
            ]}),
        );
        assert_eq!(kpis["kpi_summary"]["on_track"], json!(1));
        assert_eq!(kpis["kpi_analysis"][0]["achievement"], json!(106.67));
    }

    #[test]
    fn arr_decision_answers_with_a_verdict() {
        let out = calculate("arr-decision", json!({"arr": 18.0, "target_return": 12.0}));
        assert_eq!(out["decision"], json!("Accept"));
    }

    #[test]
    fn discounted_payback_walks_the_cash_flows() {
        let out = calculate(
            "discounted-payback",
            json!({
                "initial_investment": 1000.0,
                "cash_flows": [400.0, 400.0, 400.0],
                "discount_rate": 10.0
            }),
        );
        assert!(out.is_object());
    }

    #[test]
    fn treasury_analytics_parses_its_request() {
        let out = calculate(
            "treasury-analytics",
            json!({
                "company_id": "c-1",
                "total_cash": "250000",
                "monthly_inflow": "90000",
                "monthly_outflow": "60000",
                "short_term_debt": "40000",
                "total_debt": "120000",
                "investments": "50000",
                "fx_exposure": "10000"
            }),
        );
        assert!(out.is_object());
    }
}

fn whole(payload: &Value, key: &str) -> Result<i64, String> {
    Ok(number(payload, key)?.trunc() as i64)
}

fn optional_number(payload: &Value, key: &str, fallback: f64) -> Result<f64, String> {
    if payload.get(key).is_none() || payload.get(key) == Some(&Value::Null) {
        return Ok(fallback);
    }
    number(payload, key)
}

fn profit_calculate(payload: &Value) -> Result<Value, String> {
    to_value(&performance_measures::profit::calculate(
        number(payload, "revenue")?,
        number(payload, "total_costs")?,
    ))
}

fn profit_from_cash_flow(payload: &Value) -> Result<Value, String> {
    to_value(&performance_measures::profit::from_cash_flow(
        number(payload, "cash_inflows")?,
        number(payload, "cash_outflows")?,
    ))
}

/// An empty year list reports zero rather than refusing, which is what the service does.
fn profit_average_profit(payload: &Value) -> Result<Value, String> {
    let profits = number_list(payload, "annual_profits")?;
    Ok(serde_json::json!({
        "annual_profits": profits,
        "number_of_years": profits.len(),
        "average_profit": performance_measures::profit::average_profit(&profits),
    }))
}

fn profit_average_accounting_profit(payload: &Value) -> Result<Value, String> {
    match performance_measures::profit::average_accounting_profit(
        number(payload, "total_revenues")?,
        number(payload, "total_costs")?,
        whole(payload, "years")? as i32,
    ) {
        Some(output) => to_value(&output),
        None => Ok(serde_json::json!({"error": "Years must be positive"})),
    }
}

fn profit_net_profit(payload: &Value) -> Result<Value, String> {
    to_value(&performance_measures::profit::net_profit_after_tax(
        number(payload, "revenue")?,
        number(payload, "costs")?,
        number(payload, "tax_rate")?,
    ))
}

fn profit_compare(payload: &Value) -> Result<Value, String> {
    to_value(&performance_measures::profit::compare_profits(
        number(payload, "project_a_revenue")?,
        number(payload, "project_a_costs")?,
        number(payload, "project_b_revenue")?,
        number(payload, "project_b_costs")?,
    ))
}

fn arr_calculate(payload: &Value) -> Result<Value, String> {
    let output = capital_valuation::capital_returns::accounting_rate_of_return(
        number(payload, "initial_investment")?,
        number(payload, "residual_value")?,
        number(payload, "total_profits")?,
        whole(payload, "years")?,
    )?;
    to_value(&output)
}

fn arr_from_annual_profits(payload: &Value) -> Result<Value, String> {
    let profits = number_list(payload, "annual_profits")?;
    let output = capital_valuation::capital_returns::arr_from_annual_profits(
        &profits,
        number(payload, "initial_investment")?,
        optional_number(payload, "residual_value", 0.0)?,
    )?;
    to_value(&output)
}

fn arr_average_profit(payload: &Value) -> Result<Value, String> {
    capital_valuation::capital_returns::average_profit(
        number(payload, "total_profits")?,
        whole(payload, "years")?,
    )
}

fn arr_average_capital(payload: &Value) -> Result<Value, String> {
    Ok(capital_valuation::capital_returns::average_capital(
        number(payload, "initial_investment")?,
        number(payload, "residual_value")?,
    ))
}

/// Projects arrive as objects, and the crate ranks them by ARR; a project with no profits
/// is dropped from the ranking rather than ranked last, which is what the service does.
fn arr_compare(payload: &Value) -> Result<Value, String> {
    let projects = payload
        .get("projects")
        .and_then(Value::as_array)
        .ok_or("projects must be a list")?;
    let mut parsed = Vec::with_capacity(projects.len());
    for project in projects {
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_string();
        parsed.push((
            name,
            optional_number(project, "initial_investment", 0.0)?,
            optional_number(project, "residual_value", 0.0)?,
            project
                .get("annual_profits")
                .map(|_| number_list(project, "annual_profits"))
                .transpose()?
                .unwrap_or_default(),
        ));
    }
    to_value(&capital_valuation::capital_returns::arr_comparison(&parsed))
}

fn df_calculate(payload: &Value) -> Result<Value, String> {
    to_value(&capital_valuation::capital_returns::discount_factor(
        number(payload, "rate")?,
        whole(payload, "years")?,
    )?)
}

fn df_calculate_percent(payload: &Value) -> Result<Value, String> {
    to_value(
        &capital_valuation::capital_returns::discount_factor_percent(
            number(payload, "rate_percent")?,
            whole(payload, "years")?,
        ),
    )
}

fn df_annuity(payload: &Value) -> Result<Value, String> {
    to_value(&capital_valuation::capital_returns::annuity_factor(
        number(payload, "rate")?,
        whole(payload, "years")?,
    ))
}

fn df_annuity_percent(payload: &Value) -> Result<Value, String> {
    to_value(&capital_valuation::capital_returns::annuity_factor_percent(
        number(payload, "rate_percent")?,
        whole(payload, "years")?,
    ))
}

fn df_table(payload: &Value) -> Result<Value, String> {
    let years = number_list(payload, "years")?
        .into_iter()
        .map(|year| year.trunc() as i64)
        .collect::<Vec<_>>();
    to_value(&capital_valuation::capital_returns::discount_factor_table(
        number(payload, "base_rate")?,
        &years,
    ))
}

fn df_table_range(payload: &Value) -> Result<Value, String> {
    to_value(
        &capital_valuation::capital_returns::discount_factor_table_range(
            number(payload, "rate_start")?,
            number(payload, "rate_end")?,
            number(payload, "rate_step")?,
            whole(payload, "years")?,
        )?,
    )
}

#[cfg(test)]
mod capital_adapters {
    use super::*;
    use serde_json::json;

    fn calculate(name: &str, payload: Value) -> Value {
        let registry = calculators();
        let entry = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        entry(&payload).unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    #[test]
    fn arr_calculate_puts_the_return_at_forty_percent() {
        let out = calculate(
            "accounting-rate-return-calculate",
            json!({
                "initial_investment": 100000.0,
                "residual_value": 0.0,
                "total_profits": 60000.0,
                "years": 3
            }),
        );
        assert_eq!(out["average_annual_profit"], json!(20000.0));
        assert_eq!(out["average_investment"], json!(50000.0));
        assert_eq!(out["arr_percentage"], json!(40.0));
        assert_eq!(out["arr_decimal"], json!(0.4));
    }

    #[test]
    fn a_project_with_no_profits_is_left_out_of_the_ranking() {
        let out = calculate(
            "accounting-rate-return-compare",
            json!({"projects": [
                {"name": "A", "initial_investment": 100000.0, "residual_value": 0.0, "annual_profits": [30000.0, 30000.0]},
                {"name": "B", "initial_investment": 50000.0, "residual_value": 0.0, "annual_profits": [20000.0, 20000.0]},
                {"name": "C", "initial_investment": 1000.0, "annual_profits": []}
            ]}),
        );
        let ranked = out["project_comparison"].as_array().unwrap();
        assert_eq!(ranked.len(), 2);
        assert_eq!(out["recommended_project"], json!("B"));
    }

    #[test]
    fn the_discount_factor_table_rounds_like_the_service() {
        let out = calculate(
            "discount-factor-table",
            json!({"base_rate": 0.1, "years": [1, 2]}),
        );
        assert_eq!(out["factors"][0]["discount_factor"], json!(0.909091));
        assert_eq!(out["factors"][1]["discount_factor"], json!(0.826446));
    }

    #[test]
    fn a_zero_rate_step_is_refused_rather_than_spinning() {
        let registry = calculators();
        let entry = registry.get("discount-factor-table-range").unwrap();
        let failure = entry(&json!({
            "rate_start": 0.05,
            "rate_end": 0.1,
            "rate_step": 0.0,
            "years": 2
        }));
        assert!(failure.is_err());
    }

    #[test]
    fn a_percentage_discount_factor_is_the_decimal_one() {
        let decimal = calculate(
            "discount-factor-calculate",
            json!({"rate": 0.1, "years": 5}),
        );
        let percent = calculate(
            "discount-factor-calculate-percent",
            json!({"rate_percent": 10.0, "years": 5}),
        );
        assert_eq!(decimal["discount_factor"], percent["discount_factor"]);
    }
}

fn text(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| format!("{key} must be a string"))
}

fn number_map(
    payload: &Value,
    key: &str,
) -> Result<std::collections::BTreeMap<String, f64>, String> {
    let object = payload
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{key} must be an object of numbers"))?;
    let mut parsed = std::collections::BTreeMap::new();
    for (name, value) in object {
        let amount = match value {
            Value::Number(number) => number
                .as_f64()
                .ok_or_else(|| format!("{key}.{name} is not a number"))?,
            Value::String(text) => text
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("{key}.{name} is not a number"))?,
            _ => return Err(format!("{key}.{name} is not a number")),
        };
        parsed.insert(name.clone(), amount);
    }
    Ok(parsed)
}

fn marginal_contribution(payload: &Value) -> Result<Value, String> {
    let output = cost_accounting::marginal::contribution_analysis(
        &text(payload, "product_id")?,
        number(payload, "selling_price")?,
        number(payload, "variable_cost_per_unit")?,
        number(payload, "units_sold")?,
        optional_number(payload, "fixed_costs", 0.0)?,
    );
    to_value(&output)
}

fn marginal_forecast(payload: &Value) -> Result<Value, String> {
    let output = cost_accounting::marginal::forecast_profit(
        number(payload, "selling_price")?,
        number(payload, "variable_cost_per_unit")?,
        number(payload, "fixed_costs")?,
        number(payload, "expected_units")?,
    );
    to_value(&output)
}

fn marginal_cost_item_adapter(payload: &Value) -> Result<Value, String> {
    let output = cost_accounting::marginal::marginal_cost_item(
        &text(payload, "cost_name")?,
        &text(payload, "cost_code")?,
        number(payload, "variable_cost")?,
        number(payload, "fixed_cost")?,
        number(payload, "units")?,
        &text(payload, "cost_driver")?,
    );
    to_value(&output)
}

fn marginal_income_statement_adapter(payload: &Value) -> Result<Value, String> {
    let variable_costs = number_map(payload, "variable_costs")?;
    let output = cost_accounting::marginal::marginal_income_statement(
        &text(payload, "company_id")?,
        &text(payload, "period")?,
        number(payload, "sales_revenue")?,
        &variable_costs,
        number(payload, "fixed_costs")?,
    );
    to_value(&output)
}

#[cfg(test)]
mod marginal_adapters {
    use super::*;
    use serde_json::json;

    fn calculate(name: &str, payload: Value) -> Value {
        let registry = calculators();
        let entry = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        entry(&payload).unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    #[test]
    fn contribution_per_unit_and_profit_from_product() {
        let out = calculate(
            "marginal-costing-contribution",
            json!({
                "product_id": "p-1",
                "selling_price": 100.0,
                "variable_cost_per_unit": 60.0,
                "units_sold": 500.0,
                "fixed_costs": 5000.0
            }),
        );
        assert_eq!(out["contribution_per_unit"], json!(40.0));
        assert_eq!(out["contribution_margin_ratio"], json!(40.0));
        assert_eq!(out["total_contribution"], json!(20000.0));
        assert_eq!(out["profit_from_product"], json!(15000.0));
    }

    #[test]
    fn forecast_profit_subtracts_the_fixed_costs_once() {
        let out = calculate(
            "marginal-costing-profit-forecast",
            json!({
                "selling_price": 250.0,
                "variable_cost_per_unit": 150.0,
                "fixed_costs": 40000.0,
                "expected_units": 800.0
            }),
        );
        assert_eq!(out["total_contribution"], json!(80000.0));
        assert_eq!(out["forecast_profit"], json!(40000.0));
    }

    #[test]
    fn a_zero_price_does_not_divide_by_zero() {
        let out = calculate(
            "marginal-costing-profit-forecast",
            json!({
                "selling_price": 0.0,
                "variable_cost_per_unit": 10.0,
                "fixed_costs": 100.0,
                "expected_units": 5.0
            }),
        );
        assert_eq!(out["contribution_margin_ratio"], json!(0.0));
    }

    #[test]
    fn the_income_statement_breaks_variable_costs_down() {
        let out = calculate(
            "marginal-costing-income-statement",
            json!({
                "company_id": "c-1",
                "period": "2026-03",
                "sales_revenue": 100000.0,
                "variable_costs": {"materials": "30000", "labour": 20000.0},
                "fixed_costs": 20000.0
            }),
        );
        assert_eq!(out["total_variable_costs"], json!(50000.0));
        assert_eq!(out["contribution"], json!(50000.0));
        assert_eq!(out["profit"], json!(30000.0));
        assert_eq!(out["variable_cost_breakdown"]["materials"], json!(30000.0));
    }
}

/// The hedging services all take one request object, so the payload is parsed straight
/// into the request struct the port already expects. Every one of those structs defaults
/// its fields, which is how the services treat an omitted optional figure.
fn hedged<T>(payload: &Value, what: &str) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(payload.clone()).map_err(|error| format!("{what}: {error}"))
}

fn currency_hedging_analyze(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::fx_hedging::CurrencyHedgingRequest = hedged(payload, "exposures")?;
    to_value(&treasury_fx::fx_hedging::analyze_currency_hedge(&request))
}

fn commodity_hedging_analyze(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::hedging_analytics::CommodityHedgingRequest =
        hedged(payload, "exposures")?;
    to_value(&treasury_fx::hedging_analytics::analyze_commodity_hedge(
        &request,
    ))
}

fn futures_hedging_analyze(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::hedging_analytics::FuturesHedgingRequest =
        hedged(payload, "positions")?;
    to_value(&treasury_fx::hedging_analytics::analyze_futures_hedge(
        &request,
    ))
}

fn interest_rate_hedging_analyze(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::hedging_analytics::InterestRateHedgingRequest =
        hedged(payload, "exposures")?;
    to_value(&treasury_fx::hedging_analytics::analyze_interest_rate_hedge(&request))
}

fn cross_currency_swap_value(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::derivatives::CrossCurrencySwapRequest = hedged(payload, "swaps")?;
    to_value(&treasury_fx::derivatives::value_cross_currency_swaps(
        &request,
    ))
}

#[cfg(test)]
mod hedging_adapters {
    use super::*;
    use serde_json::json;

    fn calculate(name: &str, payload: Value) -> Value {
        let registry = calculators();
        let entry = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        entry(&payload).unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    #[test]
    fn currency_hedging_reads_its_exposures() {
        let out = calculate(
            "currency-hedging-analyze",
            json!({
                "company_id": "c-1",
                "current_hedge_ratio": 0.4,
                "target_hedge_ratio": 0.8,
                "risk_aversion": 2.0,
                "exposures": [{
                    "currency_pair": "USD/ZWL",
                    "exposure_amount": 100000.0,
                    "spot_rate": 1.0,
                    "expected_rate": 1.05,
                    "volatility": 0.12
                }]
            }),
        );
        assert_eq!(out["exposure_analysis"].as_array().unwrap().len(), 1);
        assert!(out["portfolio_metrics"].is_object());
        assert!(!out["recommendations"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_commodity_hedge_reads_its_exposures() {
        let out = calculate(
            "commodity-hedging-analyze",
            json!({
                "company_id": "c-1",
                "hedge_ratio": 0.6,
                "budget_rate": {"copper": 9000.0},
                "exposures": [{
                    "commodity": "copper",
                    "exposure_type": "purchase",
                    "volume": 100.0,
                    "unit": "tonnes",
                    "current_price": 9500.0,
                    "budget_price": 9000.0,
                    "volatility": 0.2
                }]
            }),
        );
        assert_eq!(out["exposure_analysis"].as_array().unwrap().len(), 1);
        assert!(out["hedge_plan"].as_array().is_some());
        assert!(out["budget_impact"].is_object());
    }

    #[test]
    fn an_interest_rate_hedge_reads_its_debt_exposures() {
        let out = calculate(
            "interest-rate-hedging-analyze",
            json!({
                "company_id": "c-1",
                "total_debt": 5000000.0,
                "variable_debt": 2000000.0,
                "yield_curve_slope": 0.015,
                "rate_shock_scenarios": [-0.01, 0.01, 0.02],
                "exposures": [{
                    "debt_id": "d-1",
                    "principal": 2000000.0,
                    "current_rate": 0.09,
                    "rate_type": "variable",
                    "maturity_years": 3.0,
                    "reference_rate": "ZWL prime"
                }]
            }),
        );
        assert!(out["exposure_summary"].is_object());
        assert_eq!(out["sensitivity_analysis"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn a_futures_hedge_reads_its_positions() {
        let out = calculate(
            "futures-hedging-analyze",
            json!({
                "company_id": "c-1",
                "spot_exposure": 1000000.0,
                "target_hedge_ratio": 0.75,
                "basis_risk": 0.005,
                "positions": [{
                    "future_id": "f-1",
                    "contract_month": "2027-03",
                    "position_size": 10.0,
                    "entry_price": 1.0,
                    "current_price": 1.02,
                    "contract_size": 100000.0
                }]
            }),
        );
        assert_eq!(out["position_analysis"].as_array().unwrap().len(), 1);
        assert!(out["optimal_hedge_ratio"].is_number());
    }

    #[test]
    fn a_cross_currency_swap_is_valued_per_swap() {
        let out = calculate(
            "cross-currency-swap-value",
            json!({
                "company_id": "c-1",
                "ois_rates": {"USD": 0.05},
                "fx_spot": {"USD/ZWL": 1.0},
                "swaps": [{
                    "swap_id": "s-1",
                    "notional_1": 1000000.0,
                    "currency_1": "USD",
                    "rate_1": 0.04,
                    "notional_2": 1000000.0,
                    "currency_2": "ZWL",
                    "rate_2": 0.09,
                    "tenor_years": 5.0,
                    "exchange_at_inception": false
                }]
            }),
        );
        assert_eq!(out["swap_valuations"].as_array().unwrap().len(), 1);
        assert!(out["currency_exposure"].is_object());
    }
}

fn multi_currency_convert(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::multi_currency::CurrencyConversionRequest =
        hedged(payload, "from_currency")?;
    // The service stamps the conversion with the wall clock unless the caller supplied one.
    let converted_at = request
        .conversion_date
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    to_value(&treasury_fx::multi_currency::convert_fixed(
        &request,
        &converted_at,
    ))
}

fn multi_currency_hedge(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::multi_currency::HedgeRequest = hedged(payload, "exposure_amount")?;
    let hedge_id = format!("HEDGE-{}", chrono::Utc::now().format("%Y%m%d%H%M"));
    to_value(&treasury_fx::multi_currency::create_hedge(
        &request, &hedge_id,
    ))
}

fn multi_currency_transaction(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::multi_currency::TransactionRequest =
        hedged(payload, "transaction_id")?;
    to_value(&treasury_fx::multi_currency::process_transaction(&request))
}

fn foreign_currency_translate(payload: &Value) -> Result<Value, String> {
    let request: treasury_fx::fx_translation::TranslationRequest = hedged(payload, "net_assets")?;
    to_value(&treasury_fx::fx_translation::translate_financials(&request))
}

#[cfg(test)]
mod currency_adapters {
    use super::*;
    use serde_json::json;

    fn calculate(name: &str, payload: Value) -> Value {
        let registry = calculators();
        let entry = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        entry(&payload).unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    #[test]
    fn a_known_pair_converts_at_its_table_rate_less_the_spread() {
        let out = calculate(
            "multi-currency-convert",
            json!({
                "company_id": "c-1",
                "amount": 1000.0,
                "from_currency": "USD",
                "to_currency": "EUR"
            }),
        );
        // 0.91 is the table rate, less the flat 0.2 per cent spread.
        assert_eq!(out["exchange_rate"], json!(0.91));
        assert_eq!(out["converted_amount"], json!(908.18));
    }

    #[test]
    fn an_unknown_pair_converts_at_one_less_the_spread() {
        let out = calculate(
            "multi-currency-convert",
            json!({
                "company_id": "c-1",
                "amount": 1000.0,
                "from_currency": "ZWL",
                "to_currency": "GBP"
            }),
        );
        assert_eq!(out["exchange_rate"], json!(1.0));
        assert_eq!(out["converted_amount"], json!(998.0));
    }

    #[test]
    fn a_hedge_mints_its_own_identifier() {
        let out = calculate(
            "multi-currency-hedge",
            json!({
                "company_id": "c-1",
                "exposure_amount": 100000.0,
                "currency_pair": "USD/ZWL",
                "hedge_type": "forward",
                "tenor": 3,
                "notional": 100000.0
            }),
        );
        assert!(out["hedge_id"].as_str().unwrap().starts_with("HEDGE-"));
        assert_eq!(out["strike_rate"], json!(1.1));
    }

    #[test]
    fn every_transaction_is_booked_in_usd() {
        let out = calculate(
            "multi-currency-transaction",
            json!({
                "company_id": "c-1",
                "transaction_id": "t-1",
                "amount": 5000.0,
                "currency": "ZWL",
                "transaction_type": "purchase",
                "counterparty_currency": "ZWL"
            }),
        );
        assert_eq!(out["amount_in_functional"], json!(5000.0));
        assert_eq!(out["functional_currency"], json!("USD"));
        assert_eq!(out["fx_gain_loss"], json!(50.0));
    }

    #[test]
    fn translation_reports_the_same_adjustment_twice() {
        let out = calculate(
            "foreign-currency-translation-translate",
            json!({
                "company_id": "c-1",
                "subsidiary": "Harare",
                "functional_currency": "USD",
                "presentation_currency": "ZWL",
                "closing_rate": 30.0,
                "avg_rate": 28.0,
                "historical_rate": 25.0,
                "goodwill": 0.0,
                "net_assets": [{"account": "cash", "amount": 1000.0, "is_monetary": true}],
                "income_statement": [{"account": "revenue", "amount": 500.0, "is_monetary": false}]
            }),
        );
        assert_eq!(
            out["cumulative_translation_adjustment"],
            out["exchange_differences"]
        );
        assert_eq!(out["cumulative_translation_adjustment"], json!(2000.0));
    }
}

#[cfg(test)]
mod asset_domain_doors {
    use super::*;
    use serde_json::json;

    fn calculate(name: &str, payload: Value) -> Value {
        let registry = calculators();
        let entry = registry
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        entry(&payload).unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    #[test]
    fn investment_property_answers_on_the_five_routes_the_service_published() {
        let fair_value = calculate(
            "investment-property-fair-value-model",
            json!({
                "company_id": "c-1",
                "reporting_date": "2026-10-31",
                "properties": [{"property_id": "P1", "property_name": "Block A",
                                "acquisition_cost": "800", "fair_value_last_reporting": "1000"}],
                "current_fair_values": {"P1": "1200"},
                "rental_income": "800",
                "direct_operating_expenses": "300"
            }),
        );
        assert_eq!(fair_value["accounting_model"], json!("fair_value"));
        assert_eq!(
            fair_value["total_fair_value_or_carrying_amount"],
            json!("1200.00")
        );
        assert_eq!(fair_value["fair_value_gain_loss_in_oci"], json!("200.00"));

        let cost = calculate(
            "investment-property-cost-model",
            json!({
                "company_id": "c-1",
                "reporting_date": "2026-10-31",
                "properties": [{"property_id": "P1", "property_name": "Block A",
                                "acquisition_cost": "100000", "fair_value_last_reporting": "1000"}],
                "depreciation_rate": 0.02
            }),
        );
        assert_eq!(
            cost["cost_model_depreciations"][0]["depreciation_for_period"],
            json!("2000.00")
        );

        let transfer = calculate(
            "investment-property-transfer",
            json!({
                "property_id": "P1", "reason_for_transfer": "let",
                "carrying_amount": "1000", "fair_value": "1500",
                "model_from": "owner_occupied", "model_to": "investment",
                "transfer_date": "2026-10-31"
            }),
        );
        assert_eq!(transfer["treatment"], json!("recognized in OCI"));

        let disclosure = calculate(
            "investment-property-fair-value-disclosure",
            json!({
                "properties": [{"property_id": "P1", "acquisition_cost": "900000",
                                "fair_value_last_reporting": "20000000"}],
                "current_fair_values": {}
            }),
        );
        assert_eq!(disclosure["level_3_count"], json!(1));
        assert_eq!(
            disclosure["valuation_technique"],
            json!("Discounted cash flows")
        );

        let yields = calculate(
            "investment-property-yields",
            json!({"rental_income_annual": "120000", "current_fair_value": "2000000",
                   "capitalization_rate": 6.0}),
        );
        assert_eq!(yields["gross_yield"], json!(6.0));
        assert_eq!(yields["net_yield"], json!(4.2));
    }

    #[test]
    fn biological_assets_answer_on_the_six_routes() {
        let measured = calculate(
            "biological-assets-measure",
            json!({
                "company_id": "c-1",
                "reporting_date": "2026-10-31",
                "assets": [{"asset_id": "A1", "asset_name": "Herd", "asset_category": "bearer_biological",
                            "quantity": "10", "unit": "head", "maturity_status": "mature"}],
                "transformations": [],
                "harvests": [{"quantity_harvested": "500", "selling_price_per_unit": "12.50",
                              "costs_to_sell": "300"}]
            }),
        );
        assert_eq!(measured["total_biological_assets"], json!("47500.00"));
        assert_eq!(measured["harvested_amount"], json!("6250.00"));

        let level1 = calculate(
            "biological-assets-fair-value-level1",
            json!({"asset_quantity": "100", "quoted_price_per_unit": "12.50",
                   "costs_to_sell_percentage": 0.05}),
        );
        assert_eq!(level1["fair_value"], json!("1187.50"));

        let level2 = calculate(
            "biological-assets-fair-value-level2",
            json!({"asset_quantity": "10", "market_price_similar": "1000",
                   "adjustment_factor": 0.9, "costs_to_sell_percentage": 0.1}),
        );
        assert_eq!(level2["fair_value"], json!("8100.00"));

        let level3 = calculate(
            "biological-assets-fair-value-level3",
            json!({"present_value_fcfe": "90000", "terminal_value": "15000",
                   "costs_to_sell": "500", "discount_rate": 0.12}),
        );
        assert_eq!(level3["fair_value"], json!("104500.00"));

        let classified = calculate(
            "biological-assets-consumable-bearer-distinction",
            json!({"purpose": "sale"}),
        );
        assert_eq!(classified["classification"], json!("consumable"));

        let disclosure = calculate(
            "biological-assets-disclosure-template",
            json!({"assets": [{"asset_id": "A1", "asset_category": "bearer_biological",
                               "location": "Farm A", "quantity": "1"}],
                   "reporting_date": "2026-10-31"}),
        );
        assert_eq!(disclosure["level_3_assets"], json!(1));
    }

    #[test]
    fn disposal_account_answers_on_calculate_and_post() {
        let entry = calculate(
            "disposal-account-calculate",
            json!({
                "asset_id": "8f3c1d2e-1", "asset_name": "Van", "asset_code": "MV-1",
                "disposal_date": "2026-10-31", "reason": "sold",
                "original_cost": "50000", "accumulated_depreciation": "30000",
                "disposal_proceeds": "25000"
            }),
        );
        assert_eq!(entry["profit_on_disposal"], json!("5000.00"));
        assert_eq!(entry["net_book_value"], json!("20000.00"));

        let journal = calculate("disposal-account-post", entry);
        assert_eq!(journal["reference"], json!("DISP-8f3c1d2e"));
        assert_eq!(journal["entries"].as_array().unwrap().len(), 4);
        assert_eq!(journal["entries"][3]["account_code"], json!("7100"));
    }
}
