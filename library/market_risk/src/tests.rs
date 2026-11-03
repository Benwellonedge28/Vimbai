//! Parity figures captured from the six Python services on canonical inputs.

use crate::credit::{
    analyze, financial_score, historical_pd, rate, Borrower, CreditRequest, Financials, Qualitative,
};
use crate::distress::{predict, CompanyData};
use crate::fraud::{assess, FraudAssessmentRequest};
use crate::options::{black_scholes_call, black_scholes_put, price, Option, PricingRequest};
use crate::regulatory::{calculate, RegulatoryRequest};
use crate::var::{calculate as var, Position, VarRequest};

#[test]
fn black_scholes_matches_the_service() {
    // textbook call: S=K=100, T=1, r=5%, vol=20%
    let call = black_scholes_call(100.0, 100.0, 1.0, 0.05, 0.2);
    assert!((call - 10.4506).abs() < 1e-4, "call premium {call}");
    let put = black_scholes_put(100.0, 95.0, 0.5, 0.04, 0.25);
    assert!(put > 0.0 && put < 5.0);
    // put-call parity with the same inputs
    let c = black_scholes_call(100.0, 95.0, 0.5, 0.04, 0.25);
    let p = black_scholes_put(100.0, 95.0, 0.5, 0.04, 0.25);
    let parity = c - p - (100.0 - 95.0 * (-0.04_f64 * 0.5).exp());
    assert!(parity.abs() < 1e-9, "parity residual {parity}");
}

#[test]
fn option_portfolio_matches_the_service() {
    let request = PricingRequest::new(
        "co",
        vec![
            Option::call(100.0, 100.0, 1.0, 0.2, 0.05).with_notional(1000.0),
            Option::put(90.0, 100.0, 0.5, 0.25, 0.05).with_notional(500.0),
        ],
    );
    let result = price(&request);
    assert_eq!(result.option_valuations[0]["premium"], 10.4506);
    assert_eq!(result.option_valuations[0]["delta"], 0.6368);
    assert_eq!(result.option_valuations[0]["gamma"], 0.018762);
    assert_eq!(result.option_valuations[0]["theta"], -0.0186);
    assert_eq!(result.option_valuations[0]["vega"], 0.3752);
    assert_eq!(result.option_valuations[0]["moneyness"], "ATM");
    assert_eq!(result.option_valuations[1]["premium"], 2.215);
    assert_eq!(result.option_valuations[1]["delta"], -0.2045);
    assert_eq!(result.option_valuations[1]["moneyness"], "OTM");

    assert_eq!(result.portfolio_summary["total_premium"], 11_558.09);
    assert_eq!(result.portfolio_summary["avg_premium_pct"], 7.7054);
    assert_eq!(result.portfolio_summary["option_count"], 2);
    assert_eq!(result.greeks_summary["delta"], 534.6039);
    assert_eq!(result.greeks_summary["gamma"], 26.785552);
    assert_eq!(result.greeks_summary["theta"], -26.6775);
    assert_eq!(result.greeks_summary["vega"], 475.5345);
    assert_eq!(
        result.recommendations,
        vec!["Gamma risk elevated - monitor position closely"]
    );
}

#[test]
fn in_the_money_options_report_intrinsic_value_below_the_premium() {
    let request = PricingRequest::new("co", vec![Option::call(80.0, 100.0, 1.0, 0.2, 0.05)]);
    let result = price(&request);
    let valuation = &result.option_valuations[0];
    assert_eq!(valuation["moneyness"], "ITM");
    assert_eq!(valuation["intrinsic_value"], 20.0);
    let premium = valuation["premium"].as_f64().unwrap();
    assert!(premium > 20.0, "a premium without time value is impossible");
    assert!(valuation["time_value"].as_f64().unwrap() > 0.0);
}

#[test]
fn value_at_risk_matches_the_service() {
    let request = VarRequest::new(
        "co",
        "Book",
        vec![
            Position::new("equity", 1_000_000.0, 0.2).correlated(1.0),
            Position::new("bond", 500_000.0, 0.08).correlated(0.3),
        ],
    )
    .over_days(10);
    let result = var(&request);
    assert_eq!(result.total_exposure, 1_500_000.0);
    // weighted volatility discounted by a flat 85%: the correlation input is ignored
    assert_eq!(result.portfolio_volatility, 0.136);
    assert_eq!(result.var_95, 1_061_197.14);
    assert_eq!(result.var_99, 1_500_513.4);
    assert_eq!(result.expected_shortfall, 1_330_850.88);
    assert_eq!(result.stress_loss_2sd, 408_000.0);
    assert_eq!(result.stress_loss_3sd, 612_000.0);
    assert_eq!(result.risk_level, "high");
}

#[test]
fn an_empty_portfolio_reports_no_risk_rather_than_dividing_by_zero() {
    let result = var(&VarRequest::new("co", "Empty", vec![]));
    assert_eq!(result.total_exposure, 0.0);
    assert_eq!(result.portfolio_volatility, 0.0);
    assert_eq!(result.var_95, 0.0);
    assert_eq!(result.risk_level, "low");
}

#[test]
fn distress_prediction_matches_the_service() {
    let company = CompanyData::new("co", "C", 1_000_000.0)
        .in_industry("manufacturing")
        .with_figures(
            200_000.0,
            150_000.0,
            120_000.0,
            400_000.0,
            600_000.0,
            1_500_000.0,
        );
    let result = predict(&company);
    assert_eq!(result.z1, 0.24);
    assert_eq!(result.z2, 0.21);
    assert_eq!(result.z3, 0.4);
    assert_eq!(result.z4, 0.4);
    assert_eq!(result.z5, 1.5);
    assert_eq!(result.z_score, 2.75);
    assert_eq!(result.zone, "GREY");
    assert_eq!(result.probability_distress, 0.35);
    assert_eq!(result.years_to_distress, 2.5);
    assert!(result
        .recommendation
        .starts_with("Company shows signs of stress"));
}

#[test]
fn the_manufacturing_threshold_differs_from_the_general_one() {
    let make = CompanyData::new("co", "C", 1_000_000.0)
        .in_industry("manufacturing")
        .with_figures(0.0, 0.0, 0.0, 0.0, 1.0, 2_700_000.0);
    // z = 2.7: SAFE for a trading company, GREY for a manufacturer
    assert_eq!(predict(&make).zone, "GREY");
    let mut trading = make.clone();
    trading.industry = "retail".to_string();
    assert_eq!(predict(&trading).zone, "SAFE");
}

#[test]
fn credit_risk_matches_the_service() {
    let borrower = Borrower::new("b1", "Acme", "term_loan", 1_000_000.0)
        .drawn(1_000_000.0, 0.0)
        .secured_by(400_000.0, 100_000.0);
    let financials = Financials::new(0.8, 6.0, 2.5, 300_000.0);
    let request = CreditRequest::new(borrower, financials);
    let result = analyze(&request);

    // financial 100 x 0.7 + qualitative 50 x 0.3 = 85, which is AA not AAA: the
    // qualitative base of 50 caps the composite unless the borrower scores well on
    // every soft factor too
    assert_eq!(result.credit_rating, "AA");
    assert_eq!(result.rating_grade, 2);
    assert_eq!(result.recommendation, "APPROVE");
    assert!(
        result.covenants_required.is_empty(),
        "AAA earns no covenants"
    );
    // 50% secured, capped recovery
    assert_eq!(result.lgd_calculations["recovery_rate"], 0.5);
    assert_eq!(result.lgd_calculations["loss_given_default"], 0.5);
    // term loan conversion factor is 1.0 and nothing is undrawn
    assert_eq!(result.ead_calculations["exposure_at_default"], 1_000_000.0);
    assert_eq!(result.total_expected_loss, 250.0); // 0.0005 * 0.5 * 1,000,000
}

#[test]
fn a_weak_borrower_is_declined_with_covenants() {
    let borrower = Borrower::new("b2", "Risky", "revolving", 500_000.0)
        .drawn(400_000.0, 200_000.0)
        .secured_by(0.0, 0.0);
    let financials = Financials::new(4.0, 1.0, 0.8, -50_000.0);
    let request = CreditRequest::new(borrower, financials).with_qualitative(Qualitative {
        industry_outlook: "negative".to_string(),
        management_quality: "poor".to_string(),
        market_position: "weak".to_string(),
    });
    let result = analyze(&request);
    assert_eq!(result.credit_rating, "CCC");
    assert_eq!(result.recommendation, "DECLINE");
    assert_eq!(result.covenants_required.len(), 5);
    // unsecured: recovery is zero, and the revolving factor is 0.75
    assert_eq!(result.lgd_calculations["recovery_rate"], 0.0);
    assert_eq!(result.ead_calculations["exposure_at_default"], 550_000.0);
}

#[test]
fn rating_bands_and_financial_score_match_the_service() {
    assert_eq!(rate(90.0).0, "AAA");
    assert_eq!(rate(89.9).0, "AA");
    assert_eq!(rate(50.0).0, "BB");
    assert_eq!(rate(0.0).0, "CCC");
    let strong = Financials::new(0.5, 10.0, 3.0, 500_000.0);
    assert_eq!(financial_score(&strong, 1_000_000.0), 100.0);
    let weak = Financials::new(5.0, 1.0, 0.5, -1.0);
    assert_eq!(financial_score(&weak, 1_000_000.0), 5.0);
    // free cash flow is tested against 20% of total exposure, not the drawn amount
    let borderline = Financials::new(0.5, 10.0, 3.0, 150_000.0);
    assert_eq!(financial_score(&borderline, 1_000_000.0), 90.0);
}

#[test]
fn historical_pd_matches_the_service() {
    let result = historical_pd(20, 1000, 0.95);
    assert_eq!(result.historical_pd, 0.02);
    // 2% less 1.96 binomial standard errors: 0.0087 below the estimate
    assert!((result.lower_bound - 0.011_322_8).abs() < 1e-6);
    assert!((result.adjusted_pd_for_conservatism - 0.022).abs() < 1e-12);
    let empty = historical_pd(0, 0, 0.95);
    assert_eq!(empty.historical_pd, 0.0);
    assert_eq!(empty.upper_bound, 0.0);
}

#[test]
fn fraud_risk_matches_the_service() {
    let request = FraudAssessmentRequest::new("a1", "co")
        .small_company()
        .with_history()
        .management(&["high_turnover", "aggressive_targets"])
        .control_gaps(&["segregation_of_duties", "a", "b", "c"]);
    let result = assess(&request);
    // (0.75 * 0.4 + 0.65 * 0.4 + 0.2 * 0.2) * 100
    assert_eq!(result.fraud_risk_score, 60.0);
    assert_eq!(result.risk_category, "medium");
    assert_eq!(
        result.high_risk_areas,
        vec![
            "Revenue recognition",
            "Management override",
            "Financial position manipulation"
        ]
    );
    assert_eq!(
        result.recommended_procedures,
        vec!["Standard substantive procedures"]
    );
    assert_eq!(
        result.conclusion,
        "Fraud risk assessed as medium with score of 60.0"
    );
    assert_eq!(result.pressure_indicators.len(), 3);
}

#[test]
fn a_clean_audit_reports_the_reassuring_lines_instead_of_nothing() {
    let result = assess(&FraudAssessmentRequest::new("a2", "co"));
    assert_eq!(result.fraud_risk_score, 0.0);
    assert_eq!(result.risk_category, "low");
    assert_eq!(
        result.pressure_indicators[0]["indicator"],
        "No significant pressures identified"
    );
    assert_eq!(
        result.high_risk_areas,
        vec!["No high risk areas identified"]
    );
    // financial indicators are accepted and ignored, so this stays zero
    let mut with_ratios = FraudAssessmentRequest::new("a3", "co");
    with_ratios.financial_indicators = vec![("revenue_decline".to_string(), -0.9)];
    assert_eq!(assess(&with_ratios).fraud_risk_score, 0.0);
}

#[test]
fn regulatory_ratios_match_the_service() {
    let request = RegulatoryRequest::new("co", "bank")
        .with_capital(900_000.0, 200_000.0, 1_100_000.0, 10_000_000.0)
        .with_balance_sheet(20_000_000.0, 3_000_000.0, 2_500_000.0);
    let result = calculate(&request);
    assert_eq!(result.capital_ratios[0].actual_value, 0.09);
    assert_eq!(
        result.capital_ratios[1].actual_value, 0.09,
        "CET1 is measured with Tier 1 capital"
    );
    assert_eq!(result.capital_ratios[2].actual_value, 0.11);
    assert_eq!(result.leverage_ratios[0].actual_value, 0.045);
    assert_eq!(result.liquidity_ratios[0].actual_value, 1.2);
    assert_eq!(result.overall_compliance, "compliant");
    assert_eq!(
        result.recommendations,
        vec!["Continue monitoring regulatory ratios"]
    );
}

#[test]
fn a_shortfall_flags_non_compliance_and_prompts_action() {
    let request = RegulatoryRequest::new("co", "bank")
        .with_capital(300_000.0, 50_000.0, 350_000.0, 10_000_000.0)
        .with_balance_sheet(20_000_000.0, 500_000.0, 2_500_000.0);
    let result = calculate(&request);
    assert_eq!(result.overall_compliance, "non_compliant");
    assert!(!result.capital_ratios[0].compliant);
    assert!(
        result.capital_ratios[0].buffer < 0.0,
        "the buffer goes negative"
    );
    assert_eq!(result.recommendations.len(), 3);
}
