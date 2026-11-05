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
