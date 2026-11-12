#![forbid(unsafe_code)]
//! Operational reporting calculators: bank fee analysis, KPI tracking,
//! suspense-error analysis and debt-service coverage. Ported from
//! bank-fee-analysis-service, kpi-tracking-service, suspense-error-service and
//! times-interest-earned-service.
//!
//! These are read-only diagnostics over numbers the caller already has, so they
//! belong in the safe-Rust workspace rather than in a service each.
//!
//! Kept as shipped:
//!
//! * Bank fees bucket by category in first-seen order, charge top five by
//!   amount with a stable sort (ties keep their input order), and only fire
//!   recommendations past 20% overdraft, 15% FX, 10% maintenance, 2.00 per
//!   transaction and a 0.5% fee-to-balance ratio.
//! * KPI achievement is current/target, status bands are 100% and 90%, and the
//!   alert text quotes the unrounded shortfall.
//! * Suspense effects double for a wrong original entry and for a complete
//!   reversal, and are zero for commission, principle and compensating errors,
//!   which is the exam convention the service encoded.
//! * Coverage risk is 4.0 for low, 2.0 for medium, and the covenant flags use
//!   2.5 / 1.25 / 0.25 with a flat 3.0 industry benchmark.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn r2(v: f64) -> f64 {
    py_round(v, 2)
}

fn safe_div(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

// ------------------------------------------------------- bank fee analysis

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankCharge {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub description: String,
    pub amount: f64,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeeAnalysisRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub bank_name: String,
    #[serde(default)]
    pub charges: Vec<BankCharge>,
    #[serde(default)]
    pub transaction_volume: i64,
    #[serde(default)]
    pub avg_balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeeCharge {
    pub date: String,
    pub description: String,
    pub amount: f64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeeAnalysisResult {
    pub company_id: String,
    pub period: String,
    pub bank_name: String,
    pub total_fees: f64,
    pub fee_by_category: IndexMap<String, f64>,
    pub fee_per_transaction: f64,
    pub fee_ratio_to_balance: f64,
    pub highest_charges: Vec<FeeCharge>,
    pub recommendations: Vec<String>,
}

pub fn analyze_bank_fees(req: &FeeAnalysisRequest) -> FeeAnalysisResult {
    let total: f64 = req.charges.iter().map(|c| c.amount).sum();

    let mut by_category: IndexMap<String, f64> = IndexMap::new();
    for charge in &req.charges {
        *by_category.entry(charge.category.clone()).or_insert(0.0) += charge.amount;
    }

    let per_txn = if req.transaction_volume == 0 {
        0.0
    } else {
        total / req.transaction_volume as f64
    };
    let ratio = if req.avg_balance == 0.0 {
        0.0
    } else {
        total / req.avg_balance * 100.0
    };

    // Python's sort is stable, so equal amounts keep their input order.
    let mut order: Vec<usize> = (0..req.charges.len()).collect();
    order.sort_by(|&a, &b| {
        req.charges[b]
            .amount
            .partial_cmp(&req.charges[a].amount)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let highest_charges: Vec<FeeCharge> = order
        .into_iter()
        .take(5)
        .map(|i| {
            let c = &req.charges[i];
            FeeCharge {
                date: c.date.clone(),
                description: c.description.clone(),
                amount: r2(c.amount),
                category: c.category.clone(),
            }
        })
        .collect();

    let category = |name: &str| by_category.get(name).copied().unwrap_or(0.0);

    let mut recommendations = Vec::new();
    if category("overdraft") > total * 0.2 {
        recommendations.push(
            "Overdraft fees are significant - consider arranging an overdraft facility".to_string(),
        );
    }
    if category("foreign_exchange") > total * 0.15 {
        recommendations.push(
            "High FX charges - negotiate better rates or use multi-currency accounts".to_string(),
        );
    }
    if category("maintenance") > total * 0.1 {
        recommendations.push(
            "Review account maintenance fees - negotiate waivers based on balance levels"
                .to_string(),
        );
    }
    if per_txn > 2.0 {
        recommendations.push(format!(
            "Fee per transaction ({per_txn:.2}) is high - consider bulk processing"
        ));
    }
    if ratio > 0.5 {
        recommendations.push(format!(
            "Annual fee-to-balance ratio ({ratio:.1}%) is high - switch to lower-cost banking"
        ));
    }
    if recommendations.is_empty() {
        recommendations.push("Bank fees are within acceptable ranges".to_string());
    }

    let fee_by_category: IndexMap<String, f64> =
        by_category.into_iter().map(|(k, v)| (k, r2(v))).collect();

    FeeAnalysisResult {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        bank_name: req.bank_name.clone(),
        total_fees: r2(total),
        fee_by_category,
        fee_per_transaction: r2(per_txn),
        fee_ratio_to_balance: r2(ratio),
        highest_charges,
        recommendations,
    }
}

// ----------------------------------------------------------- KPI tracking

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Kpi {
    pub kpi_id: String,
    #[serde(default)]
    pub kpi_name: String,
    pub current_value: f64,
    pub target: f64,
    #[serde(default)]
    pub previous_value: f64,
    #[serde(default)]
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KpiTrackingRequest {
    pub company_id: String,
    #[serde(default)]
    pub kpis: Vec<Kpi>,
    #[serde(default)]
    pub reporting_period: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KpiAnalysis {
    pub kpi_id: String,
    pub kpi_name: String,
    pub current: f64,
    pub target: f64,
    pub achievement: f64,
    pub unit: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KpiTrend {
    pub kpi_name: String,
    pub current: f64,
    pub previous: f64,
    pub change: f64,
    pub change_pct: f64,
    pub trend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KpiSummary {
    pub total_kpis: usize,
    pub on_track: usize,
    pub at_risk: usize,
    pub below_target: usize,
    pub overall_health: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KpiTrackingResult {
    pub company_id: String,
    pub period: String,
    pub kpi_summary: KpiSummary,
    pub kpi_analysis: Vec<KpiAnalysis>,
    pub trend_analysis: Vec<KpiTrend>,
    pub alerts: Vec<String>,
}

pub fn track_kpis(req: &KpiTrackingRequest) -> KpiTrackingResult {
    let mut kpi_analysis = Vec::new();
    let mut trend_analysis = Vec::new();
    let mut alerts = Vec::new();

    for kpi in &req.kpis {
        let achievement = safe_div(kpi.current_value, kpi.target) * 100.0;
        let change = kpi.current_value - kpi.previous_value;
        let change_pct = safe_div(change, kpi.previous_value) * 100.0;

        if achievement < 90.0 {
            alerts.push(format!(
                "{} below target by {:.1}%",
                kpi.kpi_name,
                100.0 - achievement
            ));
        }

        kpi_analysis.push(KpiAnalysis {
            kpi_id: kpi.kpi_id.clone(),
            kpi_name: kpi.kpi_name.clone(),
            current: r2(kpi.current_value),
            target: r2(kpi.target),
            achievement: r2(achievement),
            unit: kpi.unit.clone(),
            status: if achievement >= 100.0 {
                "On Track"
            } else if achievement >= 90.0 {
                "At Risk"
            } else {
                "Below Target"
            }
            .to_string(),
        });

        trend_analysis.push(KpiTrend {
            kpi_name: kpi.kpi_name.clone(),
            current: r2(kpi.current_value),
            previous: r2(kpi.previous_value),
            change: r2(change),
            change_pct: r2(change_pct),
            trend: if change > 0.0 {
                "Improving"
            } else {
                "Declining"
            }
            .to_string(),
        });
    }

    let count = |status: &str| kpi_analysis.iter().filter(|k| k.status == status).count();
    let overall_health = if kpi_analysis.is_empty() {
        0.0
    } else {
        r2(kpi_analysis.iter().map(|k| k.achievement).sum::<f64>() / kpi_analysis.len() as f64)
    };

    KpiTrackingResult {
        company_id: req.company_id.clone(),
        period: req.reporting_period.clone(),
        kpi_summary: KpiSummary {
            total_kpis: req.kpis.len(),
            on_track: count("On Track"),
            at_risk: count("At Risk"),
            below_target: count("Below Target"),
            overall_health,
        },
        kpi_analysis,
        trend_analysis,
        alerts,
    }
}

// --------------------------------------------------- debt service coverage

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TieRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    pub ebit: f64,
    pub interest_expense: f64,
    #[serde(default)]
    pub principal_repayment: f64,
    #[serde(default)]
    pub lease_payments: f64,
    #[serde(default)]
    pub operating_cash_flow: f64,
    #[serde(default)]
    pub total_debt: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverageMetrics {
    pub times_interest_earned: f64,
    pub interest_coverage_ratio: f64,
    pub debt_service_coverage_ratio: f64,
    pub fixed_charge_coverage: f64,
    pub cash_flow_to_debt: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CovenantCompliance {
    pub tie_covenant: bool,
    pub dsr_covenant: bool,
    pub debt_to_cf: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TieResult {
    pub company_id: String,
    pub period: String,
    pub coverage_metrics: CoverageMetrics,
    pub industry_benchmark: f64,
    pub risk_assessment: String,
    pub covenant_compliance: CovenantCompliance,
    pub recommendations: Vec<String>,
}

pub fn analyze_coverage(req: &TieRequest) -> TieResult {
    let tie = safe_div(req.ebit, req.interest_expense);
    let debt_service = req.interest_expense + req.principal_repayment;
    let dsr = safe_div(req.ebit, debt_service);
    let fixed_charges = req.interest_expense + req.principal_repayment + req.lease_payments;
    let fcc = safe_div(req.ebit, fixed_charges);
    let cf_to_debt = safe_div(req.operating_cash_flow, req.total_debt);

    let risk = if tie >= 4.0 {
        "low"
    } else if tie >= 2.0 {
        "medium"
    } else {
        "high"
    };

    TieResult {
        company_id: req.company_id.clone(),
        period: req.period.clone(),
        coverage_metrics: CoverageMetrics {
            times_interest_earned: r2(tie),
            interest_coverage_ratio: r2(tie),
            debt_service_coverage_ratio: r2(dsr),
            fixed_charge_coverage: r2(fcc),
            cash_flow_to_debt: py_round(cf_to_debt, 4),
        },
        industry_benchmark: 3.0,
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

    fn charge(amount: f64, category: &str) -> BankCharge {
        BankCharge {
            date: "2026-10-31".to_string(),
            description: format!("{category} charge"),
            amount,
            category: category.to_string(),
        }
    }

    #[test]
    fn fee_analysis_buckets_and_ranks() {
        let req = FeeAnalysisRequest {
            company_id: "C1".to_string(),
            period: "2026-Q3".to_string(),
            bank_name: "Bank".to_string(),
            charges: vec![
                charge(100.0, "transaction"),
                charge(60.0, "overdraft"),
                charge(40.0, "foreign_exchange"),
                charge(30.0, "maintenance"),
                charge(20.0, "wire"),
                charge(10.0, "transaction"),
            ],
            transaction_volume: 100,
            avg_balance: 50_000.0,
        };
        let out = analyze_bank_fees(&req);
        assert_eq!(out.total_fees, 260.0);
        // insertion order of first appearance
        let keys: Vec<&String> = out.fee_by_category.keys().collect();
        assert_eq!(
            keys,
            vec![
                "transaction",
                "overdraft",
                "foreign_exchange",
                "maintenance",
                "wire"
            ]
        );
        assert_eq!(out.fee_by_category["transaction"], 110.0);
        assert_eq!(out.fee_per_transaction, 2.6);
        assert_eq!(out.fee_ratio_to_balance, 0.52);
        // highest five, not six, and largest first
        assert_eq!(out.highest_charges.len(), 5);
        assert_eq!(out.highest_charges[0].amount, 100.0);
        assert_eq!(out.highest_charges[4].amount, 20.0);
    }

    #[test]
    fn fee_recommendations_fire_on_thresholds() {
        let req = FeeAnalysisRequest {
            company_id: "C1".to_string(),
            period: String::new(),
            bank_name: String::new(),
            charges: vec![
                charge(100.0, "transaction"),
                charge(60.0, "overdraft"),
                charge(40.0, "foreign_exchange"),
                charge(30.0, "maintenance"),
                charge(20.0, "wire"),
                charge(10.0, "transaction"),
            ],
            transaction_volume: 100,
            avg_balance: 50_000.0,
        };
        let out = analyze_bank_fees(&req);
        // 60/260 = 23% overdraft, 40/260 = 15.4% FX, 30/260 = 11.5% maintenance,
        // 2.60 per transaction, 0.52% of balance: all five fire.
        assert_eq!(out.recommendations.len(), 5);
        assert_eq!(
            out.recommendations[3],
            "Fee per transaction (2.60) is high - consider bulk processing"
        );
        assert_eq!(
            out.recommendations[4],
            "Annual fee-to-balance ratio (0.5%) is high - switch to lower-cost banking"
        );
    }

    #[test]
    fn cheap_banking_is_reported_as_acceptable() {
        let req = FeeAnalysisRequest {
            company_id: "C1".to_string(),
            period: String::new(),
            bank_name: String::new(),
            charges: vec![charge(10.0, "transaction")],
            transaction_volume: 100,
            avg_balance: 100_000.0,
        };
        let out = analyze_bank_fees(&req);
        assert_eq!(
            out.recommendations,
            vec!["Bank fees are within acceptable ranges".to_string()]
        );
        assert_eq!(out.fee_per_transaction, 0.1);
    }

    fn kpi(id: &str, name: &str, current: f64, target: f64, previous: f64) -> Kpi {
        Kpi {
            kpi_id: id.to_string(),
            kpi_name: name.to_string(),
            current_value: current,
            target,
            previous_value: previous,
            unit: "%".to_string(),
        }
    }

    #[test]
    fn kpi_tracking_matches_the_service() {
        let req = KpiTrackingRequest {
            company_id: "C1".to_string(),
            reporting_period: "2026-10".to_string(),
            kpis: vec![
                kpi("K1", "Margin", 32.0, 30.0, 28.0),
                kpi("K2", "Growth", 94.0, 100.0, 96.0),
                kpi("K3", "Churn", 70.0, 100.0, 0.0),
            ],
        };
        let out = track_kpis(&req);
        assert_eq!(out.kpi_analysis[0].status, "On Track");
        assert_eq!(out.kpi_analysis[0].achievement, 106.67);
        assert_eq!(out.kpi_analysis[1].status, "At Risk");
        assert_eq!(out.kpi_analysis[2].status, "Below Target");
        assert_eq!(out.kpi_summary.total_kpis, 3);
        assert_eq!(out.kpi_summary.on_track, 1);
        assert_eq!(out.kpi_summary.at_risk, 1);
        assert_eq!(out.kpi_summary.below_target, 1);
        // (106.67 + 94 + 70) / 3
        assert_eq!(out.kpi_summary.overall_health, 90.22);
        assert_eq!(out.trend_analysis[0].trend, "Improving");
        assert_eq!(out.trend_analysis[0].change_pct, 14.29);
        // zero prior value means no percentage change, not infinity
        assert_eq!(out.trend_analysis[2].change_pct, 0.0);
        assert_eq!(out.alerts, vec!["Churn below target by 30.0%".to_string()]);
    }

    #[test]
    fn coverage_metrics_and_covenants() {
        let req = TieRequest {
            company_id: "C1".to_string(),
            period: "FY2026".to_string(),
            ebit: 500.0,
            interest_expense: 100.0,
            principal_repayment: 200.0,
            lease_payments: 100.0,
            operating_cash_flow: 400.0,
            total_debt: 1000.0,
        };
        let out = analyze_coverage(&req);
        assert_eq!(out.coverage_metrics.times_interest_earned, 5.0);
        assert_eq!(out.coverage_metrics.interest_coverage_ratio, 5.0);
        assert_eq!(out.coverage_metrics.debt_service_coverage_ratio, 1.67);
        assert_eq!(out.coverage_metrics.fixed_charge_coverage, 1.25);
        assert_eq!(out.coverage_metrics.cash_flow_to_debt, 0.4);
        assert_eq!(out.industry_benchmark, 3.0);
        assert_eq!(out.risk_assessment, "low");
        assert!(out.covenant_compliance.tie_covenant);
        assert!(out.covenant_compliance.dsr_covenant);
        assert!(out.covenant_compliance.debt_to_cf);
        assert_eq!(out.recommendations[0], "Interest coverage is healthy");
    }

    #[test]
    fn thin_coverage_breaks_covenants() {
        let req = TieRequest {
            company_id: "C1".to_string(),
            period: "FY2026".to_string(),
            ebit: 150.0,
            interest_expense: 100.0,
            principal_repayment: 200.0,
            lease_payments: 0.0,
            operating_cash_flow: 100.0,
            total_debt: 1000.0,
        };
        let out = analyze_coverage(&req);
        assert_eq!(out.risk_assessment, "high");
        assert!(!out.covenant_compliance.tie_covenant);
        assert!(!out.covenant_compliance.dsr_covenant);
        assert_eq!(out.recommendations[0], "Consider debt reduction strategies");
    }

    #[test]
    fn zero_interest_reports_zero_coverage() {
        let req = TieRequest {
            company_id: "C1".to_string(),
            period: String::new(),
            ebit: 100.0,
            interest_expense: 0.0,
            principal_repayment: 0.0,
            lease_payments: 0.0,
            operating_cash_flow: 0.0,
            total_debt: 0.0,
        };
        let out = analyze_coverage(&req);
        assert_eq!(out.coverage_metrics.times_interest_earned, 0.0);
        assert_eq!(out.coverage_metrics.debt_service_coverage_ratio, 0.0);
        assert_eq!(out.coverage_metrics.cash_flow_to_debt, 0.0);
        assert_eq!(out.risk_assessment, "high");
    }
}
