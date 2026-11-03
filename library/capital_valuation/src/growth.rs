#![forbid(unsafe_code)]
//! Market value added and sustainable growth, ported from mva-service and
//! sustainable-growth-service.
//!
//! Both were single-endpoint calculators with no storage. The Python guarded
//! every division with `if denominator else 0`, so a zero denominator yields 0
//! here as well rather than an infinity or NaN.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn r4(value: f64) -> f64 {
    py_round(value, 4)
}

fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MvaInput {
    pub company_id: String,
    pub period: String,
    pub share_price: f64,
    pub shares_outstanding: f64,
    pub total_assets: f64,
    pub current_liabilities: f64,
    pub equity: f64,
    pub intangibles_book_value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MvaMetrics {
    pub market_capitalization: f64,
    pub invested_capital: f64,
    pub market_value_added: f64,
    pub mva_ratio: f64,
    pub market_to_book_ratio: f64,
    pub q_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShareholderValueAnalysis {
    pub value_created_for_shareholders: f64,
    pub return_on_market_value: f64,
    pub intangible_value_recognized: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MvaOutput {
    pub company_id: String,
    pub period: String,
    pub mva_metrics: MvaMetrics,
    pub shareholder_value_analysis: ShareholderValueAnalysis,
    pub premium_discount: String,
    pub investment_signal: String,
    pub recommendations: Vec<String>,
}

/// Market capitalisation less invested capital, with the original signal bands.
pub fn market_value_added(input: &MvaInput) -> MvaOutput {
    let market_cap = input.share_price * input.shares_outstanding;
    let invested_capital = input.total_assets - input.current_liabilities;
    let mva = market_cap - invested_capital;

    let mva_ratio = safe_div(market_cap, invested_capital);
    let market_to_book = safe_div(market_cap, input.equity);
    let q_ratio = safe_div(
        market_cap + input.intangibles_book_value,
        input.total_assets,
    );

    let premium_discount = if mva > 0.0 { "premium" } else { "discount" };
    let investment_signal = if mva_ratio > 1.5 {
        "buy"
    } else if mva_ratio > 1.0 {
        "hold"
    } else {
        "sell"
    };

    let recommendations = if premium_discount == "premium" {
        vec![
            "Market values company at a premium - growth expectations high".to_string(),
            "Focus on sustaining intangible value creation".to_string(),
            "Communicate value drivers to market".to_string(),
        ]
    } else {
        vec![
            "Market perceives value destruction".to_string(),
            "Review strategic direction and operational efficiency".to_string(),
            "Consider restructuring to unlock shareholder value".to_string(),
        ]
    };

    MvaOutput {
        company_id: input.company_id.clone(),
        period: input.period.clone(),
        mva_metrics: MvaMetrics {
            market_capitalization: r2(market_cap),
            invested_capital: r2(invested_capital),
            market_value_added: r2(mva),
            mva_ratio: r2(mva_ratio),
            market_to_book_ratio: r2(market_to_book),
            q_ratio: r2(q_ratio),
        },
        shareholder_value_analysis: ShareholderValueAnalysis {
            value_created_for_shareholders: r2(mva),
            return_on_market_value: r2(safe_div(mva, invested_capital) * 100.0),
            intangible_value_recognized: r2(market_cap - input.equity),
        },
        premium_discount: premium_discount.to_string(),
        investment_signal: investment_signal.to_string(),
        recommendations,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrowthInput {
    pub company_id: String,
    pub period: String,
    pub net_income: f64,
    pub total_equity: f64,
    pub dividends_paid: f64,
    pub target_debt_to_equity: f64,
    pub actual_debt_to_equity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrowthMetrics {
    pub return_on_equity: f64,
    pub dividend_payout_ratio: f64,
    pub retention_ratio: f64,
    pub internal_growth_rate: f64,
    pub sustainable_growth_rate: f64,
    pub achievable_growth_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrowthComparison {
    pub internal_growth: f64,
    pub sustainable_growth: f64,
    pub achievable_growth: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrowthOutput {
    pub company_id: String,
    pub period: String,
    pub growth_metrics: GrowthMetrics,
    pub growth_comparison: GrowthComparison,
    pub growth_assessment: String,
    pub recommendations: Vec<String>,
}

/// SGR = ROE x retention x (1 + target D/E), with the achievable rate scaled by
/// how much of the target leverage is actually in place. A zero target leverage
/// falls back to the internal growth rate.
pub fn sustainable_growth(input: &GrowthInput) -> GrowthOutput {
    let roe = safe_div(input.net_income, input.total_equity);
    let payout_ratio = safe_div(input.dividends_paid, input.net_income);
    let retention_ratio = 1.0 - payout_ratio;
    let internal_gr = roe * retention_ratio;
    let sustainable_gr = roe * retention_ratio * (1.0 + input.target_debt_to_equity);
    let achievable = if input.target_debt_to_equity != 0.0 {
        (input.actual_debt_to_equity / input.target_debt_to_equity).min(1.0) * sustainable_gr
    } else {
        internal_gr
    };

    let assessment = if achievable > 0.2 {
        "aggressive"
    } else if achievable > 0.1 {
        "moderate"
    } else {
        "conservative"
    };

    GrowthOutput {
        company_id: input.company_id.clone(),
        period: input.period.clone(),
        growth_metrics: GrowthMetrics {
            return_on_equity: r4(roe),
            dividend_payout_ratio: r4(payout_ratio),
            retention_ratio: r4(retention_ratio),
            internal_growth_rate: r4(internal_gr),
            sustainable_growth_rate: r4(sustainable_gr),
            achievable_growth_rate: r4(achievable),
        },
        growth_comparison: GrowthComparison {
            internal_growth: r2(internal_gr * 100.0),
            sustainable_growth: r2(sustainable_gr * 100.0),
            achievable_growth: r2(achievable * 100.0),
        },
        growth_assessment: assessment.to_string(),
        recommendations: vec![
            if assessment == "moderate" {
                "Growth strategy aligns with sustainable capacity".to_string()
            } else {
                "Consider adjusting growth targets".to_string()
            },
            "Monitor ROE sustainability".to_string(),
            "Balance dividend policy with growth investments".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mva_input() -> MvaInput {
        MvaInput {
            company_id: "c1".into(),
            period: "2026-FY".into(),
            share_price: 12.0,
            shares_outstanding: 1_000_000.0,
            total_assets: 8_000_000.0,
            current_liabilities: 2_000_000.0,
            equity: 5_000_000.0,
            intangibles_book_value: 500_000.0,
        }
    }

    #[test]
    fn mva_metrics_follow_the_service() {
        let out = market_value_added(&mva_input());
        assert_eq!(out.mva_metrics.market_capitalization, 12_000_000.0);
        assert_eq!(out.mva_metrics.invested_capital, 6_000_000.0);
        assert_eq!(out.mva_metrics.market_value_added, 6_000_000.0);
        assert_eq!(out.mva_metrics.mva_ratio, 2.0);
        assert_eq!(out.mva_metrics.market_to_book_ratio, 2.4);
        assert_eq!(out.mva_metrics.q_ratio, 1.56);
        assert_eq!(out.premium_discount, "premium");
        assert_eq!(out.investment_signal, "buy");
        assert_eq!(out.shareholder_value_analysis.return_on_market_value, 100.0);
        assert_eq!(
            out.shareholder_value_analysis.intangible_value_recognized,
            7_000_000.0
        );
        assert_eq!(out.recommendations.len(), 3);
    }

    #[test]
    fn mva_zero_denominators_do_not_explode() {
        let mut input = mva_input();
        input.total_assets = 0.0;
        input.current_liabilities = 0.0;
        input.equity = 0.0;
        let out = market_value_added(&input);
        assert_eq!(out.mva_metrics.market_value_added, 12_000_000.0);
        assert_eq!(out.mva_metrics.mva_ratio, 0.0);
        assert_eq!(out.mva_metrics.market_to_book_ratio, 0.0);
        assert_eq!(out.mva_metrics.q_ratio, 0.0);
        assert_eq!(out.investment_signal, "sell");
        assert_eq!(out.premium_discount, "premium");
    }

    #[test]
    fn sustainable_growth_bands() {
        let input = GrowthInput {
            company_id: "c1".into(),
            period: "2026-FY".into(),
            net_income: 800_000.0,
            total_equity: 5_000_000.0,
            dividends_paid: 240_000.0,
            target_debt_to_equity: 0.5,
            actual_debt_to_equity: 0.4,
        };
        let out = sustainable_growth(&input);
        assert_eq!(out.growth_metrics.return_on_equity, 0.16);
        assert_eq!(out.growth_metrics.dividend_payout_ratio, 0.3);
        assert_eq!(out.growth_metrics.retention_ratio, 0.7);
        assert_eq!(out.growth_metrics.internal_growth_rate, 0.112);
        assert_eq!(out.growth_metrics.sustainable_growth_rate, 0.168);
        // achievable is scaled by actual/target leverage but never above the SGR
        assert_eq!(out.growth_metrics.achievable_growth_rate, 0.1344);
        assert_eq!(out.growth_assessment, "moderate");
        assert_eq!(
            out.recommendations[0],
            "Growth strategy aligns with sustainable capacity"
        );
    }

    #[test]
    fn zero_target_leverage_uses_internal_growth() {
        let input = GrowthInput {
            company_id: "c1".into(),
            period: "2026-FY".into(),
            net_income: -100_000.0,
            total_equity: 5_000_000.0,
            dividends_paid: 0.0,
            target_debt_to_equity: 0.0,
            actual_debt_to_equity: 0.3,
        };
        let out = sustainable_growth(&input);
        assert_eq!(out.growth_metrics.achievable_growth_rate, -0.02);
        assert_eq!(out.growth_assessment, "conservative");
        assert_eq!(out.recommendations[0], "Consider adjusting growth targets");
    }
}
