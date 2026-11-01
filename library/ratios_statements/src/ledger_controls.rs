//! Suspense-error analysis, bank fee analysis and times-interest-earned
//! coverage.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

// ---------------------------------------------------------------- suspense

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuspenseError {
    pub error_type: String,
    pub description: String,
    pub amount: f64,
    pub account_affected: String,
    pub correct_debit: Option<String>,
    pub correct_credit: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuspenseInput {
    pub company_id: String,
    pub period: String,
    pub suspense_balance: f64,
    pub errors: Vec<SuspenseError>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuspenseCorrection {
    pub error_type: String,
    pub description: String,
    pub amount: f64,
    pub effect_on_suspense: f64,
    pub account_affected: String,
    pub corrective_action: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuspenseAnalysis {
    pub company_id: String,
    pub period: String,
    pub original_suspense_balance: f64,
    pub total_errors_found: usize,
    pub corrected_suspense_balance: f64,
    pub corrections: Vec<SuspenseCorrection>,
}

/// `/analyze`. The suspense effect is driven purely by the error type:
/// omissions and unknown types clear their own amount, original-entry and
/// complete-reversal errors clear double, and commission, principle and
/// compensating errors clear nothing because they never entered suspense.
/// The corrected balance is the original balance less the total effect, so
/// a large omission can drive it negative rather than being floored.
pub fn analyze_suspense(req: &SuspenseInput) -> SuspenseAnalysis {
    let mut total_corrections = 0.0;
    let mut corrections = Vec::new();

    for error in &req.errors {
        let amount = error.amount;
        let (effect, action) = match error.error_type.as_str() {
            "omission" => (
                amount,
                format!(
                    "Record missing entry: Dr {}, Cr {}",
                    error
                        .correct_debit
                        .clone()
                        .unwrap_or_else(|| "N/A".to_string()),
                    error
                        .correct_credit
                        .clone()
                        .unwrap_or_else(|| "N/A".to_string())
                ),
            ),
            "commission" => (0.0, "Transfer from wrong to correct account".to_string()),
            "principle" => (
                0.0,
                format!(
                    "Reclassify from {} to correct category",
                    error.account_affected
                ),
            ),
            "original_entry" => (
                amount * 2.0,
                format!("Correct understated/overstated entry by {}", amount * 2.0),
            ),
            "complete_reversal" => (
                amount * 2.0,
                "Reverse and re-enter correctly (Dr/Cr swapped)".to_string(),
            ),
            "compensating" => (
                0.0,
                "Offsetting errors - no net suspense effect".to_string(),
            ),
            _ => (amount, format!("Correct entry for {}", error.description)),
        };
        total_corrections += effect;
        corrections.push(SuspenseCorrection {
            error_type: error.error_type.clone(),
            description: error.description.clone(),
            amount: py_round2(amount),
            effect_on_suspense: py_round2(effect),
            account_affected: error.account_affected.clone(),
            corrective_action: action,
        });
    }

    let corrected_balance = req.suspense_balance - total_corrections;
    SuspenseAnalysis {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        original_suspense_balance: py_round2(req.suspense_balance),
        total_errors_found: req.errors.len(),
        corrected_suspense_balance: py_round2(corrected_balance),
        corrections,
    }
}

// --------------------------------------------------------------- bank fees

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BankCharge {
    pub date: String,
    pub description: String,
    pub amount: f64,
    pub category: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BankFeeInput {
    pub company_id: String,
    pub period: String,
    pub bank_name: String,
    pub transaction_volume: f64,
    pub avg_balance: f64,
    pub charges: Vec<BankCharge>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HighestCharge {
    pub date: String,
    pub description: String,
    pub amount: f64,
    pub category: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BankFeeAnalysis {
    pub company_id: String,
    pub period: String,
    pub bank_name: String,
    pub total_fees: f64,
    pub fee_by_category: Vec<(String, f64)>,
    pub fee_per_transaction: f64,
    pub fee_ratio_to_balance: f64,
    pub highest_charges: Vec<HighestCharge>,
    pub recommendations: Vec<String>,
}

/// `/analyze`. Category totals keep first-seen order, the top five charges
/// are sorted by amount descending, and the ratio is annualised as a
/// straight percentage of the average balance rather than a monthly rate.
pub fn analyze_bank_fees(req: &BankFeeInput) -> BankFeeAnalysis {
    let total: f64 = req.charges.iter().map(|c| c.amount).sum();

    let mut by_category: Vec<(String, f64)> = Vec::new();
    for charge in &req.charges {
        match by_category.iter_mut().find(|(k, _)| *k == charge.category) {
            Some((_, v)) => *v += charge.amount,
            None => by_category.push((charge.category.clone(), charge.amount)),
        }
    }

    let per_txn = if req.transaction_volume == 0.0 {
        0.0
    } else {
        total / req.transaction_volume
    };
    let ratio = if req.avg_balance == 0.0 {
        0.0
    } else {
        total / req.avg_balance * 100.0
    };

    let mut sorted: Vec<&BankCharge> = req.charges.iter().collect();
    sorted.sort_by(|a, b| {
        b.amount
            .partial_cmp(&a.amount)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let highest = sorted
        .into_iter()
        .take(5)
        .map(|c| HighestCharge {
            date: c.date.clone(),
            description: c.description.clone(),
            amount: py_round2(c.amount),
            category: c.category.clone(),
        })
        .collect();

    let category_total = |name: &str| -> f64 {
        by_category
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    };

    let mut recommendations = Vec::new();
    if category_total("overdraft") > total * 0.2 {
        recommendations.push(
            "Overdraft fees are significant - consider arranging an overdraft facility".to_string(),
        );
    }
    if category_total("foreign_exchange") > total * 0.15 {
        recommendations.push(
            "High FX charges - negotiate better rates or use multi-currency accounts".to_string(),
        );
    }
    if category_total("maintenance") > total * 0.1 {
        recommendations.push(
            "Review account maintenance fees - negotiate waivers based on balance levels"
                .to_string(),
        );
    }
    if per_txn > 2.0 {
        recommendations.push(format!(
            "Fee per transaction ({:.2}) is high - consider bulk processing",
            per_txn
        ));
    }
    if ratio > 0.5 {
        recommendations.push(format!(
            "Annual fee-to-balance ratio ({:.1}%) is high - switch to lower-cost banking",
            ratio
        ));
    }
    if recommendations.is_empty() {
        recommendations.push("Bank fees are within acceptable ranges".to_string());
    }

    BankFeeAnalysis {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        bank_name: req.bank_name.clone(),
        total_fees: py_round2(total),
        fee_by_category: by_category
            .into_iter()
            .map(|(k, v)| (k, py_round2(v)))
            .collect(),
        fee_per_transaction: py_round2(per_txn),
        fee_ratio_to_balance: py_round2(ratio),
        highest_charges: highest,
        recommendations,
    }
}

// ------------------------------------------------- coverage (TIE service)

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TieInput {
    pub company_id: String,
    pub period: String,
    pub ebit: f64,
    pub interest_expense: f64,
    pub principal_repayment: f64,
    pub lease_payments: f64,
    pub operating_cash_flow: f64,
    pub total_debt: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoverageMetrics {
    pub times_interest_earned: f64,
    pub interest_coverage_ratio: f64,
    pub debt_service_coverage_ratio: f64,
    pub fixed_charge_coverage: f64,
    pub cash_flow_to_debt: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CovenantCompliance {
    pub tie_covenant: bool,
    pub dsr_covenant: bool,
    pub debt_to_cf: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TieAnalysis {
    pub company_id: String,
    pub period: String,
    pub coverage_metrics: CoverageMetrics,
    pub industry_benchmark: f64,
    pub risk_assessment: String,
    pub covenant_compliance: CovenantCompliance,
    pub recommendations: Vec<String>,
}

/// `/analyze`. Each denominator is separately guarded, so a missing
/// interest expense reports zero coverage rather than an error;
/// the benchmark is a flat 3.0; the risk band is low at 4x, medium at 2x
/// and high below that; and the cash-flow-to-debt covenant is written as
/// `>= 0.25`, i.e. debt must not exceed four times operating cash flow.
pub fn analyze_times_interest_earned(req: &TieInput) -> TieAnalysis {
    let tie = if req.interest_expense == 0.0 {
        0.0
    } else {
        req.ebit / req.interest_expense
    };
    let debt_service = req.interest_expense + req.principal_repayment;
    let dsr = if debt_service == 0.0 {
        0.0
    } else {
        req.ebit / debt_service
    };
    let fixed_charges = req.interest_expense + req.principal_repayment + req.lease_payments;
    let fcc = if fixed_charges == 0.0 {
        0.0
    } else {
        req.ebit / fixed_charges
    };
    let cf_to_debt = if req.total_debt == 0.0 {
        0.0
    } else {
        req.operating_cash_flow / req.total_debt
    };
    let benchmark = 3.0;
    let risk = if tie >= 4.0 {
        "low"
    } else if tie >= 2.0 {
        "medium"
    } else {
        "high"
    };

    TieAnalysis {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        coverage_metrics: CoverageMetrics {
            times_interest_earned: py_round2(tie),
            interest_coverage_ratio: py_round2(tie),
            debt_service_coverage_ratio: py_round2(dsr),
            fixed_charge_coverage: py_round2(fcc),
            cash_flow_to_debt: round4(cf_to_debt),
        },
        industry_benchmark: benchmark,
        risk_assessment: risk.to_string(),
        covenant_compliance: CovenantCompliance {
            tie_covenant: tie >= 2.5,
            dsr_covenant: dsr >= 1.25,
            debt_to_cf: cf_to_debt >= 0.25,
        },
        recommendations: vec![
            if risk == "low" {
                "Interest coverage is healthy".to_string()
            } else {
                "Consider debt reduction strategies".to_string()
            },
            "Monitor covenant compliance".to_string(),
            "Review refinancing options for lower interest costs".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(kind: &str, amount: f64) -> SuspenseError {
        SuspenseError {
            error_type: kind.into(),
            description: format!("{} error", kind),
            amount,
            account_affected: "Suspense".into(),
            correct_debit: None,
            correct_credit: None,
        }
    }

    #[test]
    fn suspense_effects_follow_the_error_type() {
        let r = analyze_suspense(&SuspenseInput {
            company_id: "c1".into(),
            period: "2026-03".into(),
            suspense_balance: 50_000.0,
            errors: vec![
                error("omission", 5_000.0),
                error("commission", 3_000.0),
                error("principle", 2_000.0),
                error("original_entry", 4_000.0),
                error("complete_reversal", 1_000.0),
                error("compensating", 6_000.0),
            ],
        });
        assert_eq!(r.total_errors_found, 6);
        assert_eq!(r.corrections[0].effect_on_suspense, 5_000.0);
        assert_eq!(r.corrections[1].effect_on_suspense, 0.0);
        assert_eq!(r.corrections[2].effect_on_suspense, 0.0);
        assert_eq!(r.corrections[3].effect_on_suspense, 8_000.0);
        assert_eq!(r.corrections[4].effect_on_suspense, 2_000.0);
        assert_eq!(r.corrections[5].effect_on_suspense, 0.0);
        // 50,000 less (5,000 + 8,000 + 2,000)
        assert_eq!(r.corrected_suspense_balance, 35_000.0);
    }

    #[test]
    fn suspense_omission_action_names_the_accounts() {
        let r = analyze_suspense(&SuspenseInput {
            suspense_balance: 1_000.0,
            errors: vec![SuspenseError {
                correct_debit: Some("Rent".into()),
                correct_credit: Some("Bank".into()),
                ..error("omission", 500.0)
            }],
            ..Default::default()
        });
        assert_eq!(
            r.corrections[0].corrective_action,
            "Record missing entry: Dr Rent, Cr Bank"
        );
        let fallback = analyze_suspense(&SuspenseInput {
            errors: vec![error("omission", 500.0)],
            ..Default::default()
        });
        assert!(fallback.corrections[0]
            .corrective_action
            .contains("Dr N/A, Cr N/A"));
    }

    #[test]
    fn unknown_error_type_clears_its_amount_and_can_go_negative() {
        let r = analyze_suspense(&SuspenseInput {
            suspense_balance: 1_000.0,
            errors: vec![error("transposition", 4_000.0)],
            ..Default::default()
        });
        assert_eq!(r.corrections[0].effect_on_suspense, 4_000.0);
        assert_eq!(r.corrected_suspense_balance, -3_000.0);
        assert_eq!(
            r.corrections[0].corrective_action,
            "Correct entry for transposition error"
        );
    }

    fn charges() -> Vec<BankCharge> {
        vec![
            BankCharge {
                date: "2026-03-01".into(),
                description: "Overdraft interest".into(),
                amount: 4_000.0,
                category: "overdraft".into(),
            },
            BankCharge {
                date: "2026-03-04".into(),
                description: "Account fee".into(),
                amount: 3_000.0,
                category: "maintenance".into(),
            },
            BankCharge {
                date: "2026-03-09".into(),
                description: "FX on import".into(),
                amount: 1_500.0,
                category: "foreign_exchange".into(),
            },
            BankCharge {
                date: "2026-03-12".into(),
                description: "Cash deposit".into(),
                amount: 1_000.0,
                category: "cash".into(),
            },
        ]
    }

    fn fee_input() -> BankFeeInput {
        BankFeeInput {
            company_id: "c1".into(),
            period: "2026-03".into(),
            bank_name: "CBZ".into(),
            transaction_volume: 2_000.0,
            avg_balance: 2_000_000.0,
            charges: charges(),
        }
    }

    #[test]
    fn bank_fees_total_category_and_ratios() {
        let r = analyze_bank_fees(&fee_input());
        assert_eq!(r.total_fees, 9_500.0);
        assert_eq!(r.fee_by_category[0], ("overdraft".to_string(), 4_000.0));
        assert_eq!(r.fee_by_category.len(), 4);
        assert_eq!(r.fee_per_transaction, 4.75);
        // 0.475 rounds to 0.47 through the binary float, as Python does
        assert_eq!(r.fee_ratio_to_balance, 0.47);
        assert_eq!(r.highest_charges[0].amount, 4_000.0);
        assert_eq!(r.highest_charges.len(), 4);
    }

    #[test]
    fn bank_fee_recommendations_fire_on_thresholds() {
        let r = analyze_bank_fees(&fee_input());
        // overdraft 4,000 of 9,500 is over 20%; maintenance 3,000 is over 10%;
        // FX 1,500 is over 15% by one cent; 4.75 per txn is over 2
        assert_eq!(r.recommendations.len(), 4);
        assert!(r.recommendations[0].starts_with("Overdraft fees"));
        assert!(r.recommendations[1].starts_with("High FX charges"));
        assert!(r.recommendations[2].starts_with("Review account maintenance"));
        assert!(r.recommendations[3].contains("4.75"));
    }

    #[test]
    fn bank_fees_clean_bill_and_top_five_cap() {
        let mut many: Vec<BankCharge> = (0..7)
            .map(|i| BankCharge {
                date: "2026-03-01".into(),
                description: format!("fee {}", i),
                amount: (i + 1) as f64,
                category: "maintenance".into(),
            })
            .collect();
        many.push(BankCharge {
            date: "2026-03-02".into(),
            description: "tiny".into(),
            amount: 0.1,
            category: "maintenance".into(),
        });
        let r = analyze_bank_fees(&BankFeeInput {
            transaction_volume: 100_000.0,
            avg_balance: 50_000_000.0,
            charges: many,
            ..fee_input()
        });
        assert_eq!(r.highest_charges.len(), 5);
        assert_eq!(r.highest_charges[0].amount, 7.0);
        // every charge is maintenance, so that one threshold fires and no other
        assert_eq!(
            r.recommendations,
            vec![
                "Review account maintenance fees - negotiate waivers based on balance levels"
                    .to_string()
            ]
        );
    }

    fn tie_input() -> TieInput {
        TieInput {
            company_id: "c1".into(),
            period: "2026".into(),
            ebit: 1_200_000.0,
            interest_expense: 300_000.0,
            principal_repayment: 200_000.0,
            lease_payments: 100_000.0,
            operating_cash_flow: 900_000.0,
            total_debt: 3_000_000.0,
        }
    }

    #[test]
    fn coverage_metrics_and_covenants() {
        let r = analyze_times_interest_earned(&tie_input());
        assert_eq!(r.coverage_metrics.times_interest_earned, 4.0);
        assert_eq!(r.coverage_metrics.interest_coverage_ratio, 4.0);
        assert_eq!(r.coverage_metrics.debt_service_coverage_ratio, 2.4);
        assert_eq!(r.coverage_metrics.fixed_charge_coverage, 2.0);
        assert_eq!(r.coverage_metrics.cash_flow_to_debt, 0.3);
        assert_eq!(r.industry_benchmark, 3.0);
        assert_eq!(r.risk_assessment, "low");
        assert!(r.covenant_compliance.tie_covenant);
        assert!(r.covenant_compliance.dsr_covenant);
        assert!(r.covenant_compliance.debt_to_cf);
        assert_eq!(r.recommendations[0], "Interest coverage is healthy");
    }

    #[test]
    fn risk_bands_and_high_risk_advice() {
        let medium = analyze_times_interest_earned(&TieInput {
            ebit: 600_000.0,
            ..tie_input()
        });
        assert_eq!(medium.risk_assessment, "medium");
        assert_eq!(
            medium.recommendations[0],
            "Consider debt reduction strategies"
        );
        assert!(!medium.covenant_compliance.tie_covenant);

        let high = analyze_times_interest_earned(&TieInput {
            ebit: 300_000.0,
            ..tie_input()
        });
        assert_eq!(high.risk_assessment, "high");
        assert!(!high.covenant_compliance.dsr_covenant);
    }

    #[test]
    fn missing_denominators_report_zero_coverage() {
        let r = analyze_times_interest_earned(&TieInput {
            interest_expense: 0.0,
            principal_repayment: 0.0,
            lease_payments: 0.0,
            total_debt: 0.0,
            ..tie_input()
        });
        assert_eq!(r.coverage_metrics.times_interest_earned, 0.0);
        assert_eq!(r.coverage_metrics.debt_service_coverage_ratio, 0.0);
        assert_eq!(r.coverage_metrics.fixed_charge_coverage, 0.0);
        assert_eq!(r.coverage_metrics.cash_flow_to_debt, 0.0);
        assert_eq!(r.risk_assessment, "high");
        assert!(!r.covenant_compliance.debt_to_cf);
    }
}
