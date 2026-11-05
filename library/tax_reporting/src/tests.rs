//! Parity figures captured from the seven Python services on canonical inputs.
//!
//! Each expected value below is the response the service actually produced, so a
//! change in the maths fails here rather than at a client.

use crate::credit::{calculate as credit, ClaimRequest, Expenditure};
use crate::grants::{recognize, GrantRequest, GrantType};
use crate::group::{consolidate, GroupTaxRequest, Subsidiary};
use crate::provision::{calculate as provision, ProvisionRequest, TemporaryDifference};
use crate::risk::{assess, overall_level, RiskLevel, TaxRiskItem};
use crate::vat::{prepare, VatReturnRequest, VatTransaction};
use crate::xbrl::{generate, FinancialConcept, XbrlRequest};
use vimbai_common::numbers::python_float;

#[test]
fn r_and_d_credit_matches_the_service() {
    let request = ClaimRequest::new(
        "co",
        2026,
        vec![
            Expenditure::new("wages", 120_000.0, true),
            Expenditure::new("supplies", 30_000.5, true),
            Expenditure::new("overhead", 20_000.0, false),
        ],
    );
    let result = credit(&request);
    assert_eq!(result.total_expenditure, 170_000.5);
    assert_eq!(result.qualifying_expenditure, 150_000.5);
    assert_eq!(result.non_qualifying, 20_000.0);
    assert_eq!(result.credit_regular, 22_500.08);
    assert_eq!(result.credit_alternative, 30_000.1);
    assert_eq!(result.recommended_method, "alternative_simplified");
    // only qualifying lines are broken down, in first-seen order
    let categories: Vec<&String> = result.expenditure_breakdown.keys().collect();
    assert_eq!(categories, vec!["wages", "supplies"]);
    assert_eq!(result.expenditure_breakdown["wages"], 120_000.0);
}

#[test]
fn an_equal_rate_recommends_the_alternative_method() {
    // the service's tie-break, preserved: equal credits favour alternative
    let request = ClaimRequest::new("co", 2026, vec![Expenditure::new("wages", 100.0, true)])
        .with_rates(0.2, 0.2);
    assert_eq!(
        credit(&request).recommended_method,
        "alternative_simplified"
    );
    let higher = ClaimRequest::new("co", 2026, vec![Expenditure::new("wages", 100.0, true)])
        .with_rates(0.3, 0.2);
    assert_eq!(credit(&higher).recommended_method, "regular");
}

#[test]
fn group_tax_matches_the_service_and_offsets_losses() {
    let request = GroupTaxRequest::new(
        "g",
        2026,
        vec![
            Subsidiary::new("a", "A", "ZW", 400_000.0, 0.25).with_tax_paid(100_000.0),
            Subsidiary::new("b", "B", "ZW", -150_000.0, 0.25),
            Subsidiary::new("c", "C", "ZA", 60_000.0, 0.27).with_tax_paid(15_000.0),
        ],
    )
    .with_eliminations(5_000.0);

    let result = consolidate(&request);
    assert_eq!(result.consolidated_income, 305_000.0);
    assert_eq!(result.consolidated_tax, 76_250.0);
    assert_eq!(result.total_tax_already_paid, 115_000.0);
    assert_eq!(result.net_group_tax, 0.0, "already overpaid");
    assert_eq!(result.tax_savings_from_consolidation, 38_750.0);
    assert_eq!(result.subsidiary_summary.len(), 3);
    assert_eq!(result.subsidiary_summary[1]["income"], -150_000.0);
}

#[test]
fn a_group_that_has_paid_nothing_reports_no_saving() {
    // preserved quirk: the unpaid standalone shortfall cancels the standalone
    // liability, so savings stay at zero until tax is actually remitted
    let request = GroupTaxRequest::new(
        "g",
        2026,
        vec![Subsidiary::new("a", "A", "ZW", 400_000.0, 0.25)],
    );
    assert_eq!(consolidate(&request).tax_savings_from_consolidation, 0.0);
}

#[test]
fn vat_return_matches_the_service() {
    let request = VatReturnRequest::new("co", "2026-Q1")
        .with_outputs(vec![
            VatTransaction::new("sales", 100_000.0, "standard"),
            VatTransaction::new("exports", 40_000.0, "zero_rated").with_rate(0.0),
            VatTransaction::new("health", 10_000.0, "exempt"),
        ])
        .with_inputs(vec![VatTransaction::new("stock", 60_000.0, "standard")]);

    let result = prepare(&request);
    assert_eq!(
        result.total_output_vat, 16_500.0,
        "the exempt line is taxed at its rate"
    );
    assert_eq!(result.total_input_vat, 9_000.0);
    assert_eq!(result.net_vat_payable, 7_500.0);
    assert_eq!(result.net_vat_refundable, 0.0);
    assert_eq!(result.standard_rated_sales, 100_000.0);
    assert_eq!(result.zero_rated_sales, 40_000.0);
    assert_eq!(result.exempt_sales, 10_000.0);
    assert_eq!(result.standard_rated_purchases, 60_000.0);
    assert_eq!(result.vat_due_to_authority, 7_500.0);
}

#[test]
fn a_refundable_period_moves_the_balance_to_the_refund_column() {
    let request = VatReturnRequest::new("co", "2026-Q2")
        .with_outputs(vec![VatTransaction::new("sales", 10_000.0, "standard")])
        .with_inputs(vec![VatTransaction::new("plant", 100_000.0, "standard")]);
    let result = prepare(&request);
    assert_eq!(result.net_vat_payable, 0.0);
    assert_eq!(result.net_vat_refundable, 13_500.0);
    assert_eq!(result.vat_due_to_authority, 0.0);
}

#[test]
fn tax_provision_matches_the_service() {
    let request = ProvisionRequest::new("co", 2026, 500_000.0)
        .with_rate(0.25)
        .with_permanent_differences(20_000.0)
        .with_temporary_differences(vec![
            TemporaryDifference::new("accel dep", 100_000.0, 160_000.0, "taxable"),
            TemporaryDifference::new("provision", 50_000.0, 0.0, "deductible"),
        ])
        .with_loss_carryforward(80_000.0)
        .with_valuation_allowance(5_000.0);

    let result = provision(&request);
    assert_eq!(result.current_tax_expense, 82_500.0);
    assert_eq!(result.deferred_tax_assets, 12_500.0);
    assert_eq!(result.deferred_tax_liabilities, 15_000.0);
    assert_eq!(result.deferred_tax_expense, 2_500.0, "dtl - dta");
    assert_eq!(result.net_deferred_tax, -7_500.0, "dta - dtl - allowance");
    assert_eq!(result.total_tax_expense, 85_000.0);
    assert_eq!(result.effective_rate, 0.17);
    assert_eq!(result.loss_carryforward_utilized, 80_000.0);
}

#[test]
fn a_loss_making_year_uses_no_loss_allowance_and_no_current_tax() {
    let request = ProvisionRequest::new("co", 2026, -100_000.0).with_loss_carryforward(500_000.0);
    let result = provision(&request);
    assert_eq!(result.current_tax_expense, 0.0);
    assert_eq!(
        result.loss_carryforward_utilized, 0.0,
        "nothing to absorb it"
    );
    assert_eq!(result.effective_rate, 0.0);
}

#[test]
fn tax_risk_matches_the_service() {
    let risks = vec![
        TaxRiskItem::new("r1", "tp", "transfer_pricing", 600_000.0, 0.4).mitigated_by(&["docs"]),
        TaxRiskItem::new("r2", "vat", "vat_gap", 30_000.0, 0.2),
    ];
    let assessment = assess("co", "2026-11-03T00:00:00Z", &risks);
    assert_eq!(assessment.total_exposure, 630_000.0);
    assert_eq!(assessment.weighted_exposure, 246_000.0);
    assert_eq!(assessment.overall_risk_level, RiskLevel::High);
    assert_eq!(assessment.risk_count, 2);
    assert_eq!(assessment.risks[0]["weighted_score"], 240_000.0);
    assert_eq!(assessment.risks[0]["level"], "high");
    assert_eq!(assessment.risks[1]["level"], "low");
    assert_eq!(assessment.recommendations.len(), 5);
    assert_eq!(overall_level(600_000.0), RiskLevel::Critical);
    assert_eq!(overall_level(100_001.0), RiskLevel::High);
    assert_eq!(overall_level(25_001.0), RiskLevel::Medium);
    assert_eq!(overall_level(0.0), RiskLevel::Low);
}

#[test]
fn asset_grant_matches_the_service() {
    let request = GrantRequest::new("co", "Solar", 120_000.0, GrantType::Asset)
        .from_authority("MoF")
        .over_life(4)
        .against_asset(500_000.0)
        .on_conditions(&["retain jobs"]);
    let result = recognize(&request);
    assert_eq!(result.annual_recognition, 30_000.0);
    assert_eq!(result.annual_amortization, 30_000.0);
    assert_eq!(result.deferred_income_balance, 90_000.0);
    assert_eq!(result.grant_type, "asset");
    assert_eq!(result.recognition_method, "deferred_income");
    assert_eq!(
        result.journal_entries,
        vec!["Dr Bank 120000.0", "Cr Deferred Income 120000.0"]
    );
    assert_eq!(
        result.disclosure_notes,
        vec![
            "Grant received from MoF",
            "Recognized over 4 years",
            "Conditions: retain jobs"
        ]
    );
}

#[test]
fn income_grant_matches_the_service_including_its_wording() {
    let request = GrantRequest::new("co", "Wage", 50_000.0, GrantType::Income);
    let result = recognize(&request);
    assert_eq!(result.annual_recognition, 50_000.0);
    assert_eq!(result.deferred_income_balance, 0.0);
    assert_eq!(
        result.disclosure_notes,
        vec![
            "Grant received from government authority",
            "Recognized over 5 years",
            "Conditions: None specified"
        ]
    );
    assert_eq!(python_float(120_000.0), "120000.0");
    assert_eq!(python_float(30_000.5), "30000.5");
}

#[test]
fn xbrl_validation_matches_the_service() {
    let request = XbrlRequest::new("co", 2026, "E", "LEI1").with_concepts(vec![
        FinancialConcept::new("RevenueFromContracts", 1_234_567.891),
        FinancialConcept::new("Assets", 900_000.0),
    ]);
    let result = generate(&request, "2026-11-03T00:00:00Z");
    assert_eq!(result.validation_status, "invalid");
    assert_eq!(
        result.validation_errors,
        vec![
            "Missing required concept: ProfitLoss",
            "Missing required concept: Liabilities",
            "Missing required concept: Equity"
        ]
    );
    assert_eq!(result.concept_count, 2);
    assert_eq!(result.xbrl_facts[0]["value"], 1_234_567.89);
    assert_eq!(
        result.xbrl_facts[0]["taxonomy_ref"],
        "ifrs-full:RevenueFromContracts"
    );
    assert_eq!(result.filing_metadata["reporting_period"], "FY2026");
}

#[test]
fn a_complete_xbrl_filing_validates() {
    let request = XbrlRequest::new("co", 2026, "E", "LEI1").with_concepts(
        crate::xbrl::REQUIRED_CONCEPTS
            .iter()
            .map(|concept| FinancialConcept::new(concept, 100.0))
            .collect(),
    );
    let result = generate(&request, "2026-11-03T00:00:00Z");
    assert_eq!(result.validation_status, "valid");
    assert!(result.validation_errors.is_empty());
}
