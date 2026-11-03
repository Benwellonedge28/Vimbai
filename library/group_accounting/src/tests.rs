//! Parity figures captured from the six Python services on canonical inputs.

use crate::buyout::{goodwill, settle, BuyoutRequest, GoodwillRequest, RemainingPartner};
use crate::combination::{acquire, AcquisitionRequest, IdentifiableAsset};
use crate::consolidation::{
    consolidate, ConsolidationRequest, IntercompanyTransaction, ParentFinancials,
    SubsidiaryFinancials,
};
use crate::impairment::{test as test_impairment, Cgu, ImpairmentRequest, TestMethod};
use crate::partnership::{allocate, Partner, PartnershipRequest};
use crate::pension::{calculate as pension, PensionRequest};

#[test]
fn partnership_allocation_matches_the_service() {
    let request = PartnershipRequest::new(
        "co",
        "2026",
        vec![
            Partner::new("p1", "A", 200_000.0, 60.0)
                .with_salary(40_000.0)
                .with_drawings(30_000.0),
            Partner::new("p2", "B", 100_000.0, 40.0)
                .with_salary(20_000.0)
                .with_drawings(10_000.0),
        ],
        300_000.0,
    )
    .at_rate(0.05);

    let result = allocate(&request);
    assert_eq!(result.total_salary, 60_000.0);
    assert_eq!(result.total_interest, 15_000.0);
    assert_eq!(
        result.residual_profit, 225_000.0,
        "profit less salaries and interest"
    );
    assert_eq!(result.total_profit_share, 225_000.0);

    let first = &result.partner_accounts[0];
    assert_eq!(first.interest_on_capital, 10_000.0);
    assert_eq!(first.profit_share, 135_000.0);
    assert_eq!(first.closing_capital, 355_000.0);
    assert_eq!(result.partner_accounts[1].closing_capital, 205_000.0);
}

#[test]
fn a_loss_is_floored_and_the_partners_are_not_charged_it() {
    // preserved quirk: allocation never goes negative
    let request = PartnershipRequest::new(
        "co",
        "2026",
        vec![Partner::new("p1", "A", 100_000.0, 100.0)],
        -50_000.0,
    );
    let result = allocate(&request);
    assert_eq!(result.residual_profit, 0.0);
    assert_eq!(result.total_profit_share, 0.0);
    // the interest is still credited, so capital rises on a losing year
    assert_eq!(result.partner_accounts[0].closing_capital, 105_000.0);
}

#[test]
fn equal_split_when_no_shares_are_given() {
    let request = PartnershipRequest::new(
        "co",
        "2026",
        vec![
            Partner::new("p1", "A", 0.0, 0.0),
            Partner::new("p2", "B", 0.0, 0.0),
        ],
        100_000.0,
    )
    .at_rate(0.0);
    let result = allocate(&request);
    assert_eq!(result.partner_accounts[0].profit_share, 50_000.0);
    assert_eq!(result.partner_accounts[1].profit_share, 50_000.0);
}

#[test]
fn retirement_settlement_matches_the_service() {
    let request = BuyoutRequest::new("co", "p1", "A", 250_000.0, 380_000.0)
        .owing(60_000.0, 40_000.0, 20_000.0)
        .with_remaining(vec![
            RemainingPartner::new("p2", "B", 60.0),
            RemainingPartner::new("p3", "C", 40.0),
        ]);

    let result = settle(&request);
    assert_eq!(result.total_payable, 370_000.0);
    assert_eq!(result.difference, -10_000.0, "paid 10,000 above the dues");
    assert_eq!(
        result.gain_or_loss_on_retirement, 10_000.0,
        "the mirror field"
    );
    assert_eq!(
        result.remaining_partner_adjustments[0]["capital_adjustment"],
        -6_000.0
    );
    assert_eq!(
        result.remaining_partner_adjustments[1]["capital_adjustment"],
        -4_000.0
    );
}

#[test]
fn super_profit_goodwill_matches_the_service() {
    let request = GoodwillRequest::new("co", 150_000.0)
        .with_capital("p1", "A", 60.0, 200_000.0)
        .with_capital("p2", "B", 40.0, 100_000.0)
        .over_years(3)
        .at_rate(0.15);
    let result = goodwill(&request);
    assert_eq!(
        result.super_profit, 105_000.0,
        "profit above a 15% return on 300,000"
    );
    assert_eq!(result.total_goodwill, 315_000.0);
    assert_eq!(result.partner_goodwill_shares["p1"], 189_000.0);
    assert_eq!(result.partner_goodwill_shares["p2"], 126_000.0);
}

#[test]
fn acquisition_matches_the_service() {
    let request = AcquisitionRequest::new("co", "P", "S", 1_200_000.0)
        .with_assets(vec![
            IdentifiableAsset::tangible("PPE", 900_000.0),
            IdentifiableAsset::intangible("Brand", 200_000.0),
            IdentifiableAsset::liability("Debt", 250_000.0),
        ])
        .with_consideration(100_000.0, 80_000.0)
        .with_costs(50_000.0);

    let result = acquire(&request);
    assert_eq!(result.total_identifiable_assets, 1_100_000.0);
    assert_eq!(result.total_identifiable_liabilities, 250_000.0);
    assert_eq!(result.net_identifiable_assets, 850_000.0);
    assert_eq!(
        result.goodwill, 530_000.0,
        "1,380,000 consideration less 850,000"
    );
    assert_eq!(result.bargain_purchase, 0.0);
    assert_eq!(result.purchase_price_allocation[0]["allocation_pct"], 81.8);
    assert_eq!(result.purchase_price_allocation[1]["allocation_pct"], 18.2);
    // the liability line is measured against total assets too, as before
    assert_eq!(result.purchase_price_allocation[2]["allocation_pct"], 22.7);
    assert_eq!(result.purchase_price_allocation[3]["asset"], "Goodwill");
}

#[test]
fn a_bargain_purchase_reports_a_gain_instead_of_goodwill() {
    let request = AcquisitionRequest::new("co", "P", "S", 500_000.0)
        .with_assets(vec![IdentifiableAsset::tangible("PPE", 800_000.0)]);
    let result = acquire(&request);
    assert_eq!(result.goodwill, -300_000.0);
    assert_eq!(result.bargain_purchase, 300_000.0);
    let last = result.purchase_price_allocation.last().unwrap();
    assert_eq!(last["asset"], "Bargain Purchase Gain");
}

#[test]
fn impairment_matches_the_service() {
    let request = ImpairmentRequest {
        company_id: "co".to_string(),
        fiscal_year: 2026,
        cgu: Cgu::new("c1", "North", 1_000_000.0)
            .with_goodwill(150_000.0)
            .with_recoverable(780_000.0, 820_000.0),
        test_method: TestMethod::HigherOf,
    };
    let result = test_impairment(&request);
    assert_eq!(
        result.recoverable_amount, 820_000.0,
        "higher of fair value and value in use"
    );
    assert_eq!(result.impairment_loss, 180_000.0);
    assert_eq!(
        result.goodwill_impaired, 150_000.0,
        "goodwill absorbs it first"
    );
    assert_eq!(result.other_assets_impaired, 30_000.0);
    assert!(result.is_impaired);
    assert_eq!(result.post_impairment_carrying, 820_000.0);
}

#[test]
fn an_impaired_unit_that_is_worth_more_reports_no_loss() {
    let request = ImpairmentRequest {
        company_id: "co".to_string(),
        fiscal_year: 2026,
        cgu: Cgu::new("c1", "South", 500_000.0)
            .with_goodwill(50_000.0)
            .with_recoverable(600_000.0, 700_000.0),
        test_method: TestMethod::ViuOnly,
    };
    let result = test_impairment(&request);
    assert_eq!(result.recoverable_amount, 700_000.0, "value in use only");
    assert_eq!(result.impairment_loss, 0.0);
    assert!(!result.is_impaired);
    assert_eq!(result.goodwill_impaired, 0.0);
}

#[test]
fn pension_roll_forward_matches_the_service() {
    let request = PensionRequest::new("co", 2026, "Main", 1_000_000.0, 800_000.0)
        .with_costs(60_000.0, 50_000.0, 10_000.0)
        .with_benefits(40_000.0)
        .with_actuarial(15_000.0)
        .with_assets(70_000.0, 48_000.0, 55_000.0);

    let result = pension(&request);
    assert_eq!(result.dbo_end, 1_095_000.0);
    assert_eq!(result.plan_assets_end, 885_000.0);
    assert_eq!(result.funded_position, -210_000.0);
    assert_eq!(result.net_defined_benefit_liability, 210_000.0);
    assert_eq!(
        result.net_interest, 2_000.0,
        "interest cost less expected return"
    );
    assert_eq!(result.remeasurements, 22_000.0);
    assert_eq!(result.total_pension_expense, 72_000.0);
    assert_eq!(result.balance_sheet_entry["defined_benefit_asset"], 0.0);
}

#[test]
fn a_surplus_is_reported_as_an_asset_never_a_negative_liability() {
    let request = PensionRequest::new("co", 2026, "Overfunded", 500_000.0, 900_000.0);
    let result = pension(&request);
    assert_eq!(result.net_defined_benefit_liability, 0.0);
    assert_eq!(
        result.balance_sheet_entry["defined_benefit_asset"],
        400_000.0
    );
    assert_eq!(result.funded_position, 400_000.0);
}

#[test]
fn consolidation_uses_the_callers_parent_figures() {
    // The service hardcoded the parent at 10m revenue / 1.5m profit / 50m assets
    // and invented a cash flow statement. Both are gone; the parent is an input.
    let request = ConsolidationRequest::new(
        "P",
        "2026",
        ParentFinancials::new(10_000_000.0, 1_500_000.0, 50_000_000.0, 30_000_000.0),
    )
    .with_subsidiaries(vec![SubsidiaryFinancials::new(
        "s1",
        "S1",
        100.0,
        2_000_000.0,
        300_000.0,
        5_000_000.0,
        2_500_000.0,
    )
    .with_goodwill(100_000.0)])
    .with_intercompany(vec![IntercompanyTransaction::sale(
        "group sales",
        200_000.0,
    )]);

    let result = consolidate(&request);
    assert_eq!(
        result.consolidated_revenue["consolidated_revenue"],
        11_800_000.0
    );
    assert_eq!(
        result.consolidated_revenue["intercompany_sales_elimination"],
        200_000.0
    );
    assert_eq!(
        result.consolidated_profit["attributable_to_parent"],
        1_800_000.0
    );
    assert_eq!(
        result.consolidated_balance_sheet["total_assets"],
        55_000_000.0
    );
    assert_eq!(
        result.consolidated_balance_sheet["total_equity"],
        22_500_000.0
    );
    assert_eq!(result.goodwill_recognized, 100_000.0);
    assert_eq!(result.elimination_count, 1);
    // no fabricated cash flow statement is returned any more
    let as_json = serde_json::to_value(&result).unwrap();
    assert!(as_json.get("consolidated_cash_flow").is_none());
}

#[test]
fn nci_is_applied_on_top_of_the_ownership_share_a_known_defect() {
    let request = ConsolidationRequest::new(
        "P",
        "2026",
        ParentFinancials::new(1_000.0, 100.0, 5_000.0, 2_000.0),
    )
    .with_subsidiaries(vec![SubsidiaryFinancials::new(
        "s1", "S1", 60.0, 1_000.0, 200.0, 3_000.0, 1_000.0,
    )]);
    let result = consolidate(&request);
    assert_eq!(
        result.consolidated_revenue["subsidiary_revenues"]["s1"],
        600.0
    );
    // Defect preserved, not fixed: the subsidiary's profit is already scaled by
    // ownership (200 x 60% = 120) and the NCI percentage is then taken of that, so
    // the minority is charged 40% of 120 = 48 rather than 40% of 200 = 80. Group
    // profit attributable to the parent is overstated by 32 on this example. The
    // basis moves every consolidated figure ever filed, so it needs an explicit
    // decision rather than a silent port.
    assert_eq!(result.consolidated_profit["nci_share"], 48.0);
    assert_eq!(result.consolidated_profit["attributable_to_parent"], 172.0);
    // Equity is not double-applied: subsidiary equity is taken unscaled, so the
    // minority's share is a true 40% of 2,000. Only the profit line is wrong.
    assert_eq!(
        result.consolidated_balance_sheet["non_controlling_interests"],
        800.0
    );
}

#[test]
fn no_subsidiaries_means_no_nci() {
    let request = ConsolidationRequest::new(
        "P",
        "2026",
        ParentFinancials::new(1_000.0, 100.0, 5_000.0, 2_000.0),
    );
    let result = consolidate(&request);
    assert_eq!(result.consolidated_revenue["consolidated_revenue"], 1_000.0);
    assert_eq!(result.consolidated_profit["nci_share"], 0.0);
    assert_eq!(result.elimination_count, 0);
}
