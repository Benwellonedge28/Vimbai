//! IFRS 9 financial instruments: classification and measurement, the SPPI
//! test, business models, expected credit losses (general and simplified)
//! and hedge effectiveness.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FinancialAsset {
    pub asset_id: String,
    pub asset_name: String,
    /// "debt", "equity" or "derivative".
    pub asset_type: String,
    /// "amortized_cost", "FVOCI" or "FVTPL".
    pub category: String,
    pub carrying_amount: f64,
    pub face_value: f64,
    pub effective_interest_rate: f64,
    pub maturity_date: String,
    pub credit_rating: String,
    /// 1, 2 or 3.
    pub expected_credit_loss_stage: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ifrs9Request {
    pub company_id: String,
    pub reporting_date: String,
    pub financial_assets: Vec<FinancialAsset>,
    pub forward_looking_factors: Vec<(String, f64)>,
    pub base_pdg: f64,
    pub base_lgd: f64,
    pub include_macroeconomic: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub asset_id: String,
    pub business_model: String,
    pub sppi_test: String,
    pub classification: String,
    pub measurement_basis: String,
    pub reclassification_required: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EclCalculation {
    pub asset_id: String,
    pub probability_of_default: f64,
    pub loss_given_default: f64,
    pub exposure_at_default: f64,
    pub lifetime_pd: f64,
    pub stage_1_ecl: f64,
    pub stage_2_ecl: f64,
    pub stage_3_ecl: f64,
    pub impairment: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeasurementResult {
    pub asset_id: String,
    pub initial_measurement: f64,
    pub effective_interest_rate: f64,
    pub interest_revenue: f64,
    pub impairment_charge: f64,
    pub carrying_amount_end: f64,
    pub fair_value_change: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ifrs9Response {
    pub company_id: String,
    pub reporting_date: String,
    pub classifications: Vec<ClassificationResult>,
    pub ecl_calculations: Vec<EclCalculation>,
    pub measurements: Vec<MeasurementResult>,
    pub total_amortized_cost: f64,
    pub total_fvoci: f64,
    pub total_fvtpl: f64,
    pub total_ecl: f64,
    pub stage_1_ecl: f64,
    pub stage_2_ecl: f64,
    pub stage_3_ecl: f64,
}

/// `/classify`, classification plus measurement.
///
/// PYTHON QUIRKS, all preserved:
/// - the SPPI test fails ONLY for derivatives, so an equity instrument
///   passes it;
/// - the business model is read from the category, with everything that is
///   not amortized cost reported as hold-to-collect-and-sell, FVTPL
///   included;
/// - a failed SPPI never forces FVTPL: the category is echoed as both the
///   classification and the measurement basis, and reclassification is
///   always false;
/// - stage 1 books a one-year ECL and separately records a tenth of a
///   three-times-lifetime figure that the impairment maximum ignores,
///   stage 2 uses a ten-times-lifetime PD, and stage 3 uses a certain
///   default at the loss rate;
/// - impairment is the largest of the three stage figures per asset;
/// - the fair value movement is a flat two per cent of carrying amount,
///   zeroed for amortized cost, and although FVOCI assets compute it, their
///   total uses carrying amount after impairment only, while FVTPL adds it;
/// - interest revenue applies the effective rate once, with no day count.
pub fn classify_and_measure(req: &Ifrs9Request) -> Ifrs9Response {
    let mut classifications = Vec::new();
    let mut ecl_calculations = Vec::new();
    let mut measurements = Vec::new();
    let mut total_amortized = 0.0;
    let mut total_fvoci = 0.0;
    let mut total_fvtpl = 0.0;
    let mut stage_1_ecl = 0.0;
    let mut stage_2_ecl = 0.0;
    let mut stage_3_ecl = 0.0;

    for asset in &req.financial_assets {
        let sppi_result = if asset.asset_type != "derivative" {
            "passed"
        } else {
            "failed"
        };
        let business_model = if asset.category == "amortized_cost" {
            "hold_to_collect"
        } else {
            "hold_to_collect_sell"
        };
        classifications.push(ClassificationResult {
            asset_id: asset.asset_id.clone(),
            business_model: business_model.to_string(),
            sppi_test: sppi_result.to_string(),
            classification: asset.category.clone(),
            measurement_basis: asset.category.clone(),
            reclassification_required: false,
        });

        let base_pd = req.base_pdg;
        let base_lgd = req.base_lgd;
        let (pd, lifetime_pd, e1, e2, e3) = match asset.expected_credit_loss_stage {
            1 => {
                let pd = base_pd;
                let lifetime = base_pd * 3.0;
                let e1 = asset.carrying_amount * pd * base_lgd;
                let e2 = asset.carrying_amount * lifetime * base_lgd * 0.1;
                stage_1_ecl += e1;
                (pd, lifetime, e1, e2, 0.0)
            }
            2 => {
                let pd = base_pd * 2.5;
                let lifetime = base_pd * 10.0;
                let e2 = asset.carrying_amount * lifetime * base_lgd;
                stage_2_ecl += e2;
                (pd, lifetime, 0.0, e2, 0.0)
            }
            _ => {
                let e3 = asset.carrying_amount * base_lgd;
                stage_3_ecl += e3;
                (1.0, 1.0, 0.0, 0.0, e3)
            }
        };
        let impairment = e1.max(e2).max(e3);

        ecl_calculations.push(EclCalculation {
            asset_id: asset.asset_id.clone(),
            probability_of_default: pd,
            loss_given_default: base_lgd,
            exposure_at_default: asset.carrying_amount,
            lifetime_pd,
            stage_1_ecl: e1,
            stage_2_ecl: e2,
            stage_3_ecl: e3,
            impairment,
        });

        let interest_revenue = asset.carrying_amount * asset.effective_interest_rate;
        let carrying_after_impairment = asset.carrying_amount - impairment;
        let fair_value_change = asset.carrying_amount * 0.02;
        measurements.push(MeasurementResult {
            asset_id: asset.asset_id.clone(),
            initial_measurement: asset.carrying_amount,
            effective_interest_rate: asset.effective_interest_rate,
            interest_revenue,
            impairment_charge: impairment,
            carrying_amount_end: carrying_after_impairment,
            fair_value_change: if asset.category != "amortized_cost" {
                fair_value_change
            } else {
                0.0
            },
        });

        if asset.category == "amortized_cost" {
            total_amortized += carrying_after_impairment;
        } else if asset.category == "FVOCI" {
            total_fvoci += carrying_after_impairment;
        } else {
            total_fvtpl += carrying_after_impairment + fair_value_change;
        }
    }

    Ifrs9Response {
        company_id: req.company_id.clone(),
        reporting_date: req.reporting_date.clone(),
        classifications,
        ecl_calculations,
        measurements,
        total_amortized_cost: total_amortized,
        total_fvoci,
        total_fvtpl,
        total_ecl: stage_1_ecl + stage_2_ecl + stage_3_ecl,
        stage_1_ecl,
        stage_2_ecl,
        stage_3_ecl,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SppiResult {
    pub instrument_type: String,
    pub principal_amount: f64,
    pub sppi_test_passed: bool,
    pub failed_criteria: Vec<String>,
    pub classification_if_passed: String,
    pub classification_if_failed: String,
}

/// `/sppi-test`. Fails on contingent interest, a credit-sensitive feature
/// or a notional-linked return; nothing else can fail it.
pub fn perform_sppi_test(
    instrument_type: &str,
    principal_amount: f64,
    interest_type: &str,
    contingency_feature: &str,
    notional_linked: bool,
) -> SppiResult {
    let mut passed = true;
    let mut failed = Vec::new();

    if interest_type == "contingent" {
        passed = false;
        failed.push("Contingent interest payments".to_string());
    }
    if contingency_feature == "credit_sensitive" {
        passed = false;
        failed.push("Credit-sensitive payments".to_string());
    }
    if notional_linked {
        passed = false;
        failed.push("Returns linked to notional".to_string());
    }

    SppiResult {
        instrument_type: instrument_type.to_string(),
        principal_amount,
        sppi_test_passed: passed,
        failed_criteria: failed,
        classification_if_passed: "amortized_cost or FVOCI".to_string(),
        classification_if_failed: "FVTPL".to_string(),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BusinessModelAssessment {
    pub holding_purpose: String,
    pub sales_frequency: String,
    pub sales_proceeds_ratio: f64,
    pub business_model_assessed: String,
    pub measurement_basis: String,
    pub reclassification_required: bool,
}

/// `/business-model`. Frequent sales force hold-to-collect-and-sell only
/// when the purpose is not "collect_principal", which always wins, and
/// reclassification is never required.
pub fn assess_business_model(
    holding_purpose: &str,
    sales_frequency: &str,
    sales_proceeds: f64,
    total_income: f64,
) -> BusinessModelAssessment {
    let (model, measurement) = if holding_purpose == "collect_principal" {
        ("hold to collect", "amortized_cost")
    } else if holding_purpose == "collect_and_sell" || sales_frequency == "frequent" {
        ("hold to collect and sell", "FVOCI")
    } else {
        ("trading or other", "FVTPL")
    };

    BusinessModelAssessment {
        holding_purpose: holding_purpose.to_string(),
        sales_frequency: sales_frequency.to_string(),
        sales_proceeds_ratio: if total_income > 0.0 {
            sales_proceeds / total_income
        } else {
            0.0
        },
        business_model_assessed: model.to_string(),
        measurement_basis: measurement.to_string(),
        reclassification_required: false,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneralEcl {
    pub exposure_at_default: f64,
    pub probability_of_default: f64,
    pub loss_given_default: f64,
    pub undiscounted_ecl: f64,
    pub discount_rate: f64,
    pub time_to_default_years: f64,
    pub discounted_ecl: f64,
}

/// `/ecl-general`. A zero rate skips the discounting entirely rather than
/// dividing; discounting is a single division by `(1 + r)^t`.
pub fn calculate_ecl_general(
    exposure_at_default: f64,
    probability_of_default: f64,
    loss_given_default: f64,
    discount_rate: f64,
    time_to_default_years: f64,
) -> GeneralEcl {
    let undiscounted = exposure_at_default * probability_of_default * loss_given_default;
    let discounted = if discount_rate > 0.0 {
        undiscounted / (1.0 + discount_rate).powf(time_to_default_years)
    } else {
        undiscounted
    };

    GeneralEcl {
        exposure_at_default,
        probability_of_default,
        loss_given_default,
        undiscounted_ecl: undiscounted,
        discount_rate,
        time_to_default_years,
        discounted_ecl: discounted,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgeBucketEcl {
    pub bucket: String,
    pub amount: f64,
    pub loss_rate: f64,
    pub forward_adjusted_rate: f64,
    pub ecl: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimplifiedEcl {
    pub age_breakdown: Vec<AgeBucketEcl>,
    pub total_trade_receivables: f64,
    pub total_ecl: f64,
    pub ecl_coverage_ratio: f64,
}

/// `/ecl-simplified` (the provision matrix). Buckets without a historical
/// rate fall to 1%, and the forward-looking adjustment simply scales each
/// rate by `1 + adjustment` rather than shifting it.
pub fn calculate_ecl_simplified(
    ageing: &[(String, f64)],
    historical_loss_rates: &[(String, f64)],
    forward_looking_adjustment: f64,
) -> SimplifiedEcl {
    let mut breakdown = Vec::new();
    let mut total_ecl = 0.0;
    let mut total_receivables = 0.0;

    for (bucket, amount) in ageing {
        let loss_rate = historical_loss_rates
            .iter()
            .find(|(k, _)| k == bucket)
            .map(|(_, v)| *v)
            .unwrap_or(0.01);
        let adjusted_rate = loss_rate * (1.0 + forward_looking_adjustment);
        let ecl = amount * adjusted_rate;
        breakdown.push(AgeBucketEcl {
            bucket: bucket.clone(),
            amount: *amount,
            loss_rate,
            forward_adjusted_rate: adjusted_rate,
            ecl,
        });
        total_ecl += ecl;
        total_receivables += amount;
    }

    SimplifiedEcl {
        age_breakdown: breakdown,
        total_trade_receivables: total_receivables,
        total_ecl,
        ecl_coverage_ratio: if total_receivables > 0.0 {
            total_ecl / total_receivables
        } else {
            0.0
        },
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HedgeEffectiveness {
    pub hedge_type: String,
    pub hedge_ratio: f64,
    pub regression_correlation: f64,
    pub effectiveness_score: f64,
    pub qualitative_assessment: String,
    pub designated_as_hedge: bool,
    pub rebalancing_required: bool,
}

/// `/hedge-effectiveness`. The headline "score" is just the correlation
/// passed in; designation needs more than 0.8 and rebalancing applies only
/// in the 0.8 to 0.9 band, so anything above 0.9 is highly effective with
/// no rebalancing.
pub fn assess_hedge_effectiveness(
    hedge_type: &str,
    hypothetical_derivative: f64,
    actual_hedge_instrument: f64,
    regression_correlation: f64,
) -> HedgeEffectiveness {
    HedgeEffectiveness {
        hedge_type: hedge_type.to_string(),
        hedge_ratio: if hypothetical_derivative > 0.0 {
            actual_hedge_instrument / hypothetical_derivative
        } else {
            0.0
        },
        regression_correlation,
        effectiveness_score: regression_correlation,
        qualitative_assessment: if regression_correlation > 0.9 {
            "highly effective".to_string()
        } else if regression_correlation > 0.8 {
            "effective".to_string()
        } else {
            "not effective".to_string()
        },
        designated_as_hedge: regression_correlation > 0.8,
        rebalancing_required: regression_correlation > 0.8 && regression_correlation <= 0.9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(id: &str, kind: &str, category: &str, amount: f64, stage: u8) -> FinancialAsset {
        FinancialAsset {
            asset_id: id.into(),
            asset_name: format!("{id} asset"),
            asset_type: kind.into(),
            category: category.into(),
            carrying_amount: amount,
            face_value: amount,
            effective_interest_rate: 0.05,
            maturity_date: "2030-12-31".into(),
            credit_rating: "BBB".into(),
            expected_credit_loss_stage: stage,
        }
    }

    fn request(assets: Vec<FinancialAsset>) -> Ifrs9Request {
        Ifrs9Request {
            company_id: "c1".into(),
            reporting_date: "2026-12-31".into(),
            financial_assets: assets,
            forward_looking_factors: vec![],
            base_pdg: 0.01,
            base_lgd: 0.45,
            include_macroeconomic: true,
        }
    }

    #[test]
    fn stage_one_ecl_uses_one_year_pd() {
        let r = classify_and_measure(&request(vec![asset(
            "A1",
            "debt",
            "amortized_cost",
            1_000_000.0,
            1,
        )]));
        let e = &r.ecl_calculations[0];
        // 1,000,000 * 0.01 * 0.45 = 4,500
        assert_eq!(e.impairment, 4_500.0);
        assert_eq!(e.probability_of_default, 0.01);
        assert_eq!(e.lifetime_pd, 0.03);
        // the lifetime stub is recorded but does not drive the impairment
        // ca * 0.03 * 0.45 * 0.1 = 1,350, still below the one-year figure
        assert_eq!(e.stage_2_ecl, 1_350.0);
        assert_eq!(r.stage_1_ecl, 4_500.0);
    }

    #[test]
    fn stage_two_uses_ten_year_factor() {
        let r = classify_and_measure(&request(vec![asset(
            "A2",
            "debt",
            "amortized_cost",
            1_000_000.0,
            2,
        )]));
        let e = &r.ecl_calculations[0];
        assert_eq!(e.probability_of_default, 0.025);
        assert_eq!(e.lifetime_pd, 0.1);
        // 1,000,000 * 0.10 * 0.45 = 45,000
        assert_eq!(e.impairment, 45_000.0);
        assert_eq!(r.stage_2_ecl, 45_000.0);
    }

    #[test]
    fn stage_three_is_certain_default_at_lgd() {
        let r = classify_and_measure(&request(vec![asset("A3", "debt", "FVOCI", 200_000.0, 3)]));
        let e = &r.ecl_calculations[0];
        assert_eq!(e.probability_of_default, 1.0);
        assert_eq!(e.impairment, 90_000.0);
        assert_eq!(r.stage_3_ecl, 90_000.0);
    }

    #[test]
    fn equity_passes_sppi_and_failed_sppi_does_not_reclassify() {
        let r = classify_and_measure(&request(vec![
            asset("E1", "equity", "amortized_cost", 100_000.0, 1),
            asset("D1", "derivative", "amortized_cost", 100_000.0, 1),
        ]));
        let equity = &r.classifications[0];
        assert_eq!(equity.sppi_test, "passed");
        // a derivative fails the test yet stays at amortized cost
        let derivative = &r.classifications[1];
        assert_eq!(derivative.sppi_test, "failed");
        assert_eq!(derivative.classification, "amortized_cost");
        assert_eq!(derivative.measurement_basis, "amortized_cost");
        assert!(!derivative.reclassification_required);
        // FVTPL still reports a hold-to-collect-and-sell model
        let fvtpl =
            classify_and_measure(&request(vec![asset("T1", "debt", "FVTPL", 100_000.0, 1)]));
        assert_eq!(
            fvtpl.classifications[0].business_model,
            "hold_to_collect_sell"
        );
    }

    #[test]
    fn totals_treat_the_fair_value_move_inconsistently() {
        let r = classify_and_measure(&request(vec![
            asset("A1", "debt", "amortized_cost", 1_000_000.0, 1),
            asset("A2", "debt", "FVOCI", 500_000.0, 1),
            asset("A3", "equity", "FVTPL", 300_000.0, 1),
        ]));
        // interest revenue is a single application of the effective rate
        assert_eq!(r.measurements[0].interest_revenue, 50_000.0);
        assert_eq!(r.measurements[0].fair_value_change, 0.0);
        assert_eq!(r.measurements[1].fair_value_change, 10_000.0);

        assert_eq!(r.total_amortized_cost, 995_500.0);
        // FVOCI total excludes the 10,000 move it computed
        assert_eq!(r.total_fvoci, 497_750.0);
        // FVTPL total includes its 6,000 move on an impaired 298,650
        assert_eq!(r.total_fvtpl, 298_650.0 + 6_000.0);
    }

    #[test]
    fn totals_roll_up_by_category() {
        let r = classify_and_measure(&request(vec![
            asset("A1", "debt", "amortized_cost", 1_000_000.0, 1),
            asset("A2", "debt", "FVOCI", 500_000.0, 2),
        ]));
        assert_eq!(r.total_ecl, 4_500.0 + 22_500.0);
        assert_eq!(r.total_ecl, r.stage_1_ecl + r.stage_2_ecl + r.stage_3_ecl);
    }

    #[test]
    fn sppi_criteria() {
        let clean = perform_sppi_test("bond", 1_000_000.0, "fixed", "none", false);
        assert!(clean.sppi_test_passed);
        assert!(clean.failed_criteria.is_empty());
        assert_eq!(clean.classification_if_passed, "amortized_cost or FVOCI");

        let messy = perform_sppi_test(
            "structured",
            1_000_000.0,
            "contingent",
            "credit_sensitive",
            true,
        );
        assert!(!messy.sppi_test_passed);
        assert_eq!(messy.failed_criteria.len(), 3);
        assert!(messy
            .failed_criteria
            .contains(&"Contingent interest payments".to_string()));
        assert_eq!(messy.classification_if_failed, "FVTPL");

        let variable = perform_sppi_test("bond", 500_000.0, "variable", "none", false);
        assert!(variable.sppi_test_passed);
    }

    #[test]
    fn business_model_rules() {
        let hold = assess_business_model("collect_principal", "rare", 0.0, 1_000_000.0);
        assert_eq!(hold.measurement_basis, "amortized_cost");
        assert_eq!(hold.business_model_assessed, "hold to collect");
        assert!(!hold.reclassification_required);

        let sell = assess_business_model("collect_and_sell", "occasional", 200_000.0, 1_000_000.0);
        assert_eq!(sell.measurement_basis, "FVOCI");
        assert_eq!(sell.sales_proceeds_ratio, 0.2);

        // a stated collect-principal purpose wins even with frequent sales
        let frequent =
            assess_business_model("collect_principal", "frequent", 400_000.0, 1_000_000.0);
        assert_eq!(frequent.measurement_basis, "amortized_cost");

        let trading = assess_business_model("trading", "occasional", 0.0, 1_000_000.0);
        assert_eq!(trading.measurement_basis, "FVTPL");

        let no_income = assess_business_model("trading", "occasional", 500.0, 0.0);
        assert_eq!(no_income.sales_proceeds_ratio, 0.0);
    }

    #[test]
    fn general_ecl_discounting() {
        let disc = calculate_ecl_general(1_000_000.0, 0.02, 0.45, 0.05, 3.0);
        assert_eq!(disc.undiscounted_ecl, 9_000.0);
        assert!((disc.discounted_ecl - 9_000.0 / 1.05f64.powi(3)).abs() < 1e-9);
        assert!(disc.discounted_ecl < disc.undiscounted_ecl);

        let flat = calculate_ecl_general(1_000_000.0, 0.02, 0.45, 0.0, 3.0);
        assert_eq!(flat.discounted_ecl, flat.undiscounted_ecl);
    }

    #[test]
    fn simplified_matrix_defaults_and_forwards() {
        let ageing = vec![
            ("current".to_string(), 500_000.0),
            ("30_days".to_string(), 200_000.0),
            ("90_days".to_string(), 100_000.0),
        ];
        let rates = vec![
            ("current".to_string(), 0.005),
            ("30_days".to_string(), 0.02),
        ];
        let r = calculate_ecl_simplified(&ageing, &rates, 0.2);

        assert_eq!(r.total_trade_receivables, 800_000.0);
        assert_eq!(r.age_breakdown[0].forward_adjusted_rate, 0.006);
        assert_eq!(r.age_breakdown[0].ecl, 3_000.0);
        assert_eq!(r.age_breakdown[1].ecl, 200_000.0 * 0.024);
        // unknown bucket falls to a 1% base rate, then forwards to 1.2%
        assert_eq!(r.age_breakdown[2].loss_rate, 0.01);
        assert_eq!(r.age_breakdown[2].ecl, 1_200.0);
        assert_eq!(r.total_ecl, 3_000.0 + 4_800.0 + 1_200.0);
        assert!((r.ecl_coverage_ratio - 9_000.0 / 800_000.0).abs() < 1e-12);

        let empty = calculate_ecl_simplified(&[], &[], 0.2);
        assert_eq!(empty.total_ecl, 0.0);
        assert_eq!(empty.ecl_coverage_ratio, 0.0);
    }

    #[test]
    fn hedge_effectiveness_bands() {
        let high = assess_hedge_effectiveness("cash_flow", 1_000_000.0, 1_050_000.0, 0.95);
        assert_eq!(high.hedge_ratio, 1.05);
        assert_eq!(high.qualitative_assessment, "highly effective");
        assert!(high.designated_as_hedge);
        assert!(!high.rebalancing_required);
        assert_eq!(high.effectiveness_score, 0.95);

        let band = assess_hedge_effectiveness("fair_value", 1_000_000.0, 900_000.0, 0.85);
        assert_eq!(band.qualitative_assessment, "effective");
        assert!(band.designated_as_hedge);
        assert!(band.rebalancing_required);

        let fails = assess_hedge_effectiveness("net_investment", 1_000_000.0, 700_000.0, 0.75);
        assert_eq!(fails.qualitative_assessment, "not effective");
        assert!(!fails.designated_as_hedge);
        // exactly 0.8 is not designated
        let edge = assess_hedge_effectiveness("cash_flow", 1_000_000.0, 800_000.0, 0.8);
        assert!(!edge.designated_as_hedge);
        let no_notional = assess_hedge_effectiveness("cash_flow", 0.0, 500_000.0, 0.99);
        assert_eq!(no_notional.hedge_ratio, 0.0);
    }
}
