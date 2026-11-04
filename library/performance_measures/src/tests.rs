//! Parity figures captured from the six Python services on canonical inputs.

use vimbai_common::Money;

use crate::controls::{test as test_controls, Control, DeficiencyLevel, TestingRequest};
use crate::fx::{convert, hedge, rate_of, ConversionRequest, HedgeRequest};
use crate::inventory::{
    calculate as value, PurchaseRecord, SaleRecord, ValuationMethod, ValuationRequest,
};
use crate::profit::{
    average_accounting_profit, average_profit, calculate as profit, from_cash_flow,
};
use crate::realizable::{net_realizable_value, residual_value, scrap_value};
use crate::scorecard::{score, Kpi, ScorecardRequest};

#[test]
fn scorecard_matches_the_service_including_its_scale_error() {
    let request = ScorecardRequest::new(
        "co",
        "2026-Q1",
        vec![
            Kpi::new("Revenue growth", "financial", 1_000_000.0, 800_000.0).weighted(2.0),
            Kpi::new("Margin", "financial", 20.0, 25.0),
            Kpi::new("NPS", "customer", 50.0, 30.0),
            Kpi::new("Cycle time", "internal", 10.0, 9.0),
            Kpi::new("Training hours", "learning_growth", 0.0, 5.0),
        ],
    );
    let result = score(&request);
    // perspective scores are percentages multiplied by 100 again
    assert_eq!(result.financial_score, 8_666.7);
    assert_eq!(result.customer_score, 6_000.0);
    assert_eq!(result.internal_score, 9_000.0);
    assert_eq!(result.learning_score, 10_000.0);
    assert_eq!(result.overall_score, 8_416.7);
    assert_eq!(
        result.action_items[0],
        "Excellent performance across all perspectives"
    );
    assert_eq!(
        result.action_items[1],
        "NPS: Below target (60%) - review strategy"
    );
    assert_eq!(
        result.perspectives["financial"]["kpis"][0]["achievement_pct"],
        80.0
    );
    // beating a target earns no extra credit
    assert_eq!(
        result.perspectives["financial"]["kpis"][1]["achievement_pct"],
        100.0
    );
}

#[test]
fn an_empty_perspective_scores_zero_and_a_low_overall_warns() {
    let request = ScorecardRequest::new(
        "co",
        "2026-Q2",
        vec![Kpi::new("Margin", "financial", 20.0, 4.0)],
    );
    let result = score(&request);
    assert_eq!(result.customer_score, 0.0);
    assert_eq!(result.overall_score, 500.0);
    // Preserved quirk: because a perspective score is a percentage multiplied by
    // 100 again, the 90-point "excellent" threshold is cleared by a card whose only
    // KPI reaches 20% of target, and the 70-point warning can never fire. Wording
    // kept so dashboards do not move; the scale is what needs deciding.
    assert_eq!(
        result.action_items[0],
        "Excellent performance across all perspectives"
    );
    assert_eq!(
        result.action_items[1],
        "Margin: Below target (20%) - review strategy"
    );
    assert_eq!(
        result.perspectives["customer"]["kpis"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn controls_matches_the_service() {
    let request = TestingRequest::new(
        "co",
        2026,
        vec![
            Control::new("c1", "Bank rec", "preventive", "cash").tested(40, 0),
            Control::new("c2", "Approvals", "detective", "p2p").tested(40, 2),
            Control::new("c3", "Segregation", "preventive", "p2p").tested(40, 4),
            Control::new("c4", "Reconcile", "detective", "gl").tested(20, 5),
        ],
    );
    let result = test_controls(&request);
    assert_eq!(result.effective_controls, 1);
    assert_eq!(
        result.deficient_controls, 3,
        "significant and material both count"
    );
    assert_eq!(result.significant_deficiencies, 1);
    assert_eq!(result.material_weaknesses, 1);
    assert_eq!(
        result.overall_assessment,
        "Adverse opinion - material weaknesses identified"
    );
    let levels: Vec<&str> = result
        .control_details
        .iter()
        .map(|d| d["deficiency_level"].as_str().unwrap())
        .collect();
    assert_eq!(
        levels,
        vec![
            "none",
            "deficiency",
            "significant_deficiency",
            "material_weakness"
        ]
    );
    assert_eq!(result.remediation_plan.len(), 3);
    assert_eq!(
        result.remediation_plan[2]["recommended_action"],
        "Redesign Reconcile - increase sample testing frequency"
    );
}

#[test]
fn deficiency_bands_are_inclusive_at_the_boundaries() {
    assert_eq!(
        DeficiencyLevel::from_exception_rate(0.0),
        DeficiencyLevel::None
    );
    assert_eq!(
        DeficiencyLevel::from_exception_rate(0.05),
        DeficiencyLevel::Deficiency
    );
    assert_eq!(
        DeficiencyLevel::from_exception_rate(0.0501),
        DeficiencyLevel::Significant
    );
    assert_eq!(
        DeficiencyLevel::from_exception_rate(0.10),
        DeficiencyLevel::Significant
    );
    assert_eq!(
        DeficiencyLevel::from_exception_rate(0.1001),
        DeficiencyLevel::Material
    );
    // an untested control counts as effective, because a zero sample cannot fail
    let request = TestingRequest::new(
        "co",
        2026,
        vec![Control::new("c1", "Not tested", "preventive", "gl").tested(0, 0)],
    );
    assert_eq!(test_controls(&request).effective_controls, 1);
}

#[test]
fn profit_matches_the_service_unrounded() {
    let result = profit(250_000.0, 180_000.0);
    assert_eq!(result.profit, 70_000.0);
    // the service returned the raw ratio
    assert!((result.profit_margin - 28.0).abs() < 1e-9);
    // a zero revenue reports no margin rather than dividing by it
    assert_eq!(profit(0.0, 500.0).profit_margin, 0.0);
    assert_eq!(from_cash_flow(900.0, 400.0).net_cash_flow, 500.0);
    assert_eq!(average_profit(&[100_000.0, 200_000.0]), 150_000.0);
    assert_eq!(average_profit(&[]), 0.0);
}

#[test]
fn average_accounting_profit_matches_the_service() {
    let result = average_accounting_profit(1_000_000.0, 700_000.0, 3).expect("valid years");
    assert_eq!(result.average_annual_profit, 100_000.0);
    assert_eq!(result.years, 3);
    assert!(average_accounting_profit(1.0, 1.0, 0).is_none());
}

#[test]
fn inventory_methods_match_the_service() {
    let base = || {
        ValuationRequest::new("co", ValuationMethod::Fifo)
            .with_opening(Money::from_i64(5_000), 50)
            .with_purchases(vec![
                PurchaseRecord::new("2026-02-01", 100, Money::from_i64(110)),
                PurchaseRecord::new("2026-01-15", 100, Money::from_i64(100)),
            ])
            .with_sales(vec![SaleRecord::new(
                "2026-03-01",
                120,
                Money::from_i64(200),
            )])
    };

    let fifo = value(&ValuationRequest {
        method: ValuationMethod::Fifo,
        ..base()
    });
    assert_eq!(
        fifo.cost_of_goods_sold, 12_000.0,
        "opening stock goes first"
    );
    assert_eq!(fifo.ending_inventory, 14_000.0);
    assert_eq!(fifo.ending_qty, 130);
    assert_eq!(fifo.gross_profit, 12_000.0);
    assert_eq!(fifo.gross_margin, 50.0);

    let lifo = value(&ValuationRequest {
        method: ValuationMethod::Lifo,
        ..base()
    });
    assert_eq!(
        lifo.cost_of_goods_sold, 13_000.0,
        "newest purchases go first"
    );
    assert_eq!(lifo.ending_inventory, 13_000.0);
    assert_eq!(lifo.gross_margin, 45.83);

    let average = value(&ValuationRequest {
        method: ValuationMethod::WeightedAverage,
        ..base()
    });
    assert_eq!(average.total_goods_available, 26_000.0);
    assert_eq!(average.total_units_available, 250);
    assert_eq!(average.cost_of_goods_sold, 12_480.0);
    assert_eq!(average.ending_inventory, 13_520.0);
    assert_eq!(average.gross_margin, 48.0);
}

#[test]
fn realizable_values_match_the_service() {
    let nrv = net_realizable_value(500.0, 120.0, 30.0);
    assert_eq!(nrv.net_realizable_value, 350.0);
    assert_eq!(nrv.formula, "500.0 - 120.0 - 30.0 = 350.0");

    let scrap = scrap_value(100_000.0, 60_000.0, 2_000.0, 30_000.0);
    assert_eq!(scrap.net_book_value, 40_000.0);
    assert_eq!(scrap.scrap_value, 28_000.0);
    assert_eq!(scrap.gain_loss_on_disposal, -12_000.0);

    let residual = residual_value(100_000.0, 0.25, 5);
    assert_eq!(residual.total_depreciation, 125_000.0);
    assert_eq!(residual.residual_value, 0.0, "floored, never negative");
}

#[test]
fn fx_conversion_matches_the_service() {
    let request = ConversionRequest::new("USD", "ZAR", 1_000.0).on("2026-03-01");
    let result = convert(&request, "2026-11-03").expect("known currencies");
    assert_eq!(result.rate, 18.5);
    assert_eq!(result.converted_amount, 18_500.0);
    assert_eq!(result.rate_date, "2026-03-01");
    // an empty date falls back to the caller's date
    let undated =
        convert(&ConversionRequest::new("USD", "EUR", 10.0), "2026-11-03").expect("known");
    assert_eq!(undated.rate_date, "2026-11-03");
    assert_eq!(undated.converted_amount, 9.2);
    assert_eq!(rate_of("ZZZ"), None);
    assert!(convert(&ConversionRequest::new("USD", "ZZZ", 1.0), "2026-11-03").is_err());
}

#[test]
fn hedge_matches_the_service_including_its_scale_error() {
    let request = HedgeRequest::new("co", "EUR", 400_000.0, 0.75).at_forward(0.95);
    let result = hedge(&request).expect("known currency");
    assert_eq!(result.hedged_amount, 300_000.0);
    assert_eq!(result.unhedged_amount, 100_000.0);
    assert_eq!(result.hedge_cost_estimate, 600.0, "0.2% of the hedged part");
    // a 3.26% move on 100,000 unhedged reports 326,086.96, not 3,260.87
    assert_eq!(result.potential_loss_unhedged, 326_086.96);
}

#[test]
fn hedging_without_a_forward_rate_uses_the_spot_rate() {
    let request = HedgeRequest::new("co", "GBP", 100_000.0, 1.0);
    let result = hedge(&request).expect("known currency");
    assert_eq!(result.forward_rate, 0.79);
    assert_eq!(result.potential_loss_unhedged, 0.0);
    assert!(hedge(&HedgeRequest::new("co", "ZZZ", 1.0, 0.5)).is_err());
}
