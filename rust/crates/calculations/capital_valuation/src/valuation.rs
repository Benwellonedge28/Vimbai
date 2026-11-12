#![forbid(unsafe_code)]
//! Business valuation, ported from business-valuation-service.
//!
//! Four calculators plus the composite `/value` endpoint. Kept as shipped:
//!
//! * DCF projects revenue from `base_revenue * (1 + cagr)^year`, derives EBITDA,
//!   depreciation, EBIT, tax, NOPAT and capex, and discounts each year's free
//!   cash flow. The terminal value is a Gordon growth on the final year's FCF.
//! * Comparable analysis averages EV/EBITDA over comparables with positive
//!   EBITDA and P/E over comparables with positive net income, defaulting to
//!   10x and 15x when a sample is empty, then splits the difference between the
//!   EBITDA-based and earnings-based equity value.
//! * The dividend discount model is a Gordon growth on the company's own DPS
//!   with a hard-coded 12% required return and 5% growth.
//! * The weighted value applies 0.4/0.4/0.2 to whichever methods produced a
//!   per-share figure, and the range is min * 0.85 to max * 1.15. The weights are
//!   not renormalised, so a run without comparables is understated.
//!
//! Two service bugs are corrected here and pinned by tests:
//!
//! * `implied_ebitda_multiple` read `request.base_revenue`, a field that only
//!   exists on the DCF inputs, so a DCF valuation raised `AttributeError` and
//!   the endpoint returned 500. It now uses the DCF's own `base_revenue`.
//! * An empty set of method results raised `ValueError` on `min([])`. It now
//!   reports a zero range.

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

// ------------------------------------------------------------- inputs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompanyFinancials {
    pub company_id: String,
    pub company_name: String,
    pub fiscal_year_end: String,
    pub revenue: f64,
    pub ebitda: f64,
    pub ebit: f64,
    pub net_income: f64,
    pub total_assets: f64,
    pub total_equity: f64,
    pub total_debt: f64,
    pub cash: f64,
    pub shares_outstanding: f64,
    pub share_price: f64,
    #[serde(default = "default_beta")]
    pub beta: f64,
    #[serde(default)]
    pub dividend_per_share: f64,
}

fn default_beta() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcfInputs {
    pub projection_years: usize,
    pub base_revenue: f64,
    pub revenue_cagr: f64,
    pub ebitda_margin: f64,
    pub depreciation_rate: f64,
    pub capex_rate: f64,
    pub tax_rate: f64,
    pub discount_rate: f64,
    pub terminal_growth_rate: f64,
    pub net_debt: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparableCompany {
    pub company_name: String,
    pub ticker: String,
    pub ev: f64,
    pub ebitda: f64,
    pub revenue: f64,
    pub total_equity: f64,
    pub net_income: f64,
    pub shares_outstanding: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValuationRequest {
    pub company: CompanyFinancials,
    #[serde(default = "yes")]
    pub include_dcf: bool,
    #[serde(default = "yes")]
    pub include_comparables: bool,
    #[serde(default = "yes")]
    pub include_ddm: bool,
    #[serde(default)]
    pub dcf_inputs: Option<DcfInputs>,
    #[serde(default)]
    pub comparables: Vec<ComparableCompany>,
}

fn yes() -> bool {
    true
}

// ------------------------------------------------------------ outputs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcfResult {
    pub present_value_of_projections: f64,
    pub terminal_value: f64,
    pub present_value_of_terminal_value: f64,
    pub enterprise_value: f64,
    pub less_net_debt: f64,
    pub equity_value: f64,
    pub value_per_share: f64,
    pub implied_ebitda_multiple: f64,
    pub implied_pe_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparableMultiples {
    #[serde(rename = "EV/EBITDA")]
    pub ev_ebitda: f64,
    #[serde(rename = "P/E")]
    pub pe: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparableResult {
    pub method: String,
    pub multiples: ComparableMultiples,
    pub adjusted_equity_value: f64,
    pub value_per_share: f64,
    pub premium_discount_to_comps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DdmResult {
    pub value_per_share: f64,
    pub dividend_growth_rate: f64,
    pub required_return: f64,
    pub valuation_vs_market: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValuationOutput {
    pub company_id: String,
    pub valuation_date: String,
    pub dcf_valuation: Option<DcfResult>,
    pub comparable_valuation: Option<ComparableResult>,
    pub dividend_discount_valuation: Option<DdmResult>,
    pub weighted_average_value: f64,
    pub value_range_low: f64,
    pub value_range_high: f64,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaccInput {
    pub market_cap: f64,
    pub total_debt: f64,
    pub cost_of_equity: f64,
    pub cost_of_debt: f64,
    pub tax_rate: f64,
    #[serde(default = "debt_premium_default")]
    pub debt_premium: f64,
}

fn debt_premium_default() -> f64 {
    0.02
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaccOutput {
    pub market_cap: f64,
    pub total_debt: f64,
    pub total_capital: f64,
    pub equity_weight: f64,
    pub debt_weight: f64,
    pub cost_of_equity: f64,
    pub cost_of_debt: f64,
    pub after_tax_cost_of_debt: f64,
    pub wacc: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapmInput {
    pub risk_free_rate: f64,
    pub market_return: f64,
    pub beta: f64,
    #[serde(default)]
    pub size_premium: f64,
    #[serde(default)]
    pub company_specific_premium: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapmOutput {
    pub risk_free_rate: f64,
    pub market_return: f64,
    pub beta: f64,
    pub market_risk_premium: f64,
    pub size_premium: f64,
    pub company_specific_premium: f64,
    pub cost_of_equity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrecedentInput {
    pub transaction_price: f64,
    pub target_ebitda: f64,
    pub target_revenue: f64,
    pub premium_paid: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrecedentOutput {
    pub transaction_price: f64,
    pub target_ebitda: f64,
    pub target_revenue: f64,
    pub implied_ev_ebitda: f64,
    pub implied_ev_revenue: f64,
    pub premium_paid: f64,
    pub control_premium: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookValueInput {
    pub total_assets: f64,
    pub intangible_assets: f64,
    pub total_liabilities: f64,
    pub preferred_equity: f64,
    pub minority_interests: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookValueOutput {
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub intangible_assets: f64,
    pub tangible_book_value: f64,
    pub book_value_of_equity: f64,
    pub preferred_equity: f64,
    pub minority_interests: f64,
}

// ------------------------------------------------------------ calculators

/// Discounted cash flow per share and enterprise value.
pub fn dcf_valuation(company: &CompanyFinancials, dcf: &DcfInputs) -> DcfResult {
    let mut pv_projections = 0.0;
    let mut last_fcf = 0.0;

    for year in 1..=dcf.projection_years {
        let factor = (1.0 + dcf.revenue_cagr).powi(year as i32);
        let revenue = dcf.base_revenue * factor;
        let ebitda = revenue * dcf.ebitda_margin;
        let depreciation = revenue * dcf.depreciation_rate;
        let ebit = ebitda - depreciation;
        let taxes = ebit * dcf.tax_rate;
        let nopat = ebit - taxes;
        let capex = revenue * dcf.capex_rate;
        let fcf = nopat + depreciation - capex;

        pv_projections += safe_div(fcf, (1.0 + dcf.discount_rate).powi(year as i32));
        last_fcf = fcf;
    }

    let terminal_fcf = last_fcf * (1.0 + dcf.terminal_growth_rate);
    let terminal_value = safe_div(terminal_fcf, dcf.discount_rate - dcf.terminal_growth_rate);
    let pv_terminal = safe_div(
        terminal_value,
        (1.0 + dcf.discount_rate).powi(dcf.projection_years as i32),
    );

    let enterprise_value = pv_projections + pv_terminal;
    let equity_value = enterprise_value - dcf.net_debt;
    let value_per_share = safe_div(equity_value, company.shares_outstanding);

    let ebitda_scale = dcf.base_revenue * dcf.ebitda_margin;

    DcfResult {
        present_value_of_projections: pv_projections,
        terminal_value,
        present_value_of_terminal_value: pv_terminal,
        enterprise_value,
        less_net_debt: dcf.net_debt,
        equity_value,
        value_per_share,
        implied_ebitda_multiple: if ebitda_scale > 0.0 {
            enterprise_value / ebitda_scale
        } else {
            0.0
        },
        implied_pe_ratio: if company.net_income > 0.0 {
            equity_value / company.net_income
        } else {
            0.0
        },
    }
}

/// Comparable company analysis: mean EV/EBITDA and P/E, averaged equity value.
pub fn comparable_valuation(
    company: &CompanyFinancials,
    comparables: &[ComparableCompany],
) -> ComparableResult {
    let mut ev_multiples = Vec::new();
    let mut pe_multiples = Vec::new();

    for comp in comparables {
        if comp.ebitda > 0.0 {
            ev_multiples.push(comp.ev / comp.ebitda);
        }
        if comp.net_income > 0.0 {
            pe_multiples.push(comp.total_equity / comp.net_income);
        }
    }

    let avg_ev_ebitda = if ev_multiples.is_empty() {
        10.0
    } else {
        ev_multiples.iter().sum::<f64>() / ev_multiples.len() as f64
    };
    let avg_pe = if pe_multiples.is_empty() {
        15.0
    } else {
        pe_multiples.iter().sum::<f64>() / pe_multiples.len() as f64
    };

    let comp_ev = company.ebitda * avg_ev_ebitda;
    let comp_equity = company.net_income * avg_pe;
    let adjusted_equity = (comp_equity + comp_ev - company.total_debt) / 2.0;

    ComparableResult {
        method: "Comparable Company Analysis".to_string(),
        multiples: ComparableMultiples {
            ev_ebitda: avg_ev_ebitda,
            pe: avg_pe,
        },
        adjusted_equity_value: adjusted_equity,
        value_per_share: safe_div(adjusted_equity, company.shares_outstanding),
        premium_discount_to_comps: 0.0,
    }
}

/// Gordon growth dividend discount model on a 12% required return, 5% growth.
pub fn dividend_discount(company: &CompanyFinancials) -> DdmResult {
    let required_return = 0.12;
    let growth_rate = 0.05;
    let value = company.dividend_per_share * (1.0 + growth_rate) / (required_return - growth_rate);

    DdmResult {
        value_per_share: value,
        dividend_growth_rate: growth_rate,
        required_return,
        valuation_vs_market: safe_div(value - company.share_price, company.share_price) * 100.0,
    }
}

/// The composite `/value` endpoint.
pub fn value_business(request: &ValuationRequest, valuation_date: &str) -> ValuationOutput {
    let company = &request.company;

    let dcf_result = match (&request.dcf_inputs, request.include_dcf) {
        (Some(dcf), true) => Some(dcf_valuation(company, dcf)),
        _ => None,
    };

    let comparable_result = if request.include_comparables && !request.comparables.is_empty() {
        Some(comparable_valuation(company, &request.comparables))
    } else {
        None
    };

    let ddm_result = if request.include_ddm {
        Some(dividend_discount(company))
    } else {
        None
    };

    let mut weighted_value = 0.0;
    let mut values: Vec<f64> = Vec::new();
    if let Some(ref dcf) = dcf_result {
        weighted_value += dcf.value_per_share * 0.4;
        values.push(dcf.value_per_share);
    }
    if let Some(ref comps) = comparable_result {
        weighted_value += comps.value_per_share * 0.4;
        values.push(comps.value_per_share);
    }
    if let Some(ref ddm) = ddm_result {
        weighted_value += ddm.value_per_share * 0.2;
        values.push(ddm.value_per_share);
    }

    let (low, high) = match (
        values.iter().copied().fold(None::<f64>, |acc, v| {
            Some(match acc {
                Some(a) if a < v => a,
                _ => v,
            })
        }),
        values.iter().copied().fold(None::<f64>, |acc, v| {
            Some(match acc {
                Some(a) if a > v => a,
                _ => v,
            })
        }),
    ) {
        (Some(min), Some(max)) => (min * 0.85, max * 1.15),
        _ => (0.0, 0.0),
    };

    let price = company.share_price;
    let recommendation = if weighted_value > price * 1.1 {
        "BUY"
    } else if weighted_value > price * 0.9 {
        "HOLD"
    } else {
        "SELL"
    };

    ValuationOutput {
        company_id: company.company_id.clone(),
        valuation_date: valuation_date.to_string(),
        dcf_valuation: dcf_result,
        comparable_valuation: comparable_result,
        dividend_discount_valuation: ddm_result,
        weighted_average_value: weighted_value * company.shares_outstanding,
        value_range_low: low * company.shares_outstanding,
        value_range_high: high * company.shares_outstanding,
        recommendation: recommendation.to_string(),
    }
}

pub fn calculate_wacc(input: &WaccInput) -> WaccOutput {
    let total_capital = input.market_cap + input.total_debt;
    let equity_weight = safe_div(input.market_cap, total_capital);
    let debt_weight = safe_div(input.total_debt, total_capital);
    let after_tax_cost_debt = input.cost_of_debt * (1.0 - input.tax_rate);
    let wacc = equity_weight * input.cost_of_equity + debt_weight * after_tax_cost_debt;

    WaccOutput {
        market_cap: input.market_cap,
        total_debt: input.total_debt,
        total_capital,
        equity_weight,
        debt_weight,
        cost_of_equity: input.cost_of_equity,
        cost_of_debt: input.cost_of_debt,
        after_tax_cost_of_debt: after_tax_cost_debt,
        wacc,
    }
}

pub fn calculate_capm(input: &CapmInput) -> CapmOutput {
    let market_risk_premium = input.market_return - input.risk_free_rate;
    let cost_of_equity = input.risk_free_rate
        + input.beta * market_risk_premium
        + input.size_premium
        + input.company_specific_premium;

    CapmOutput {
        risk_free_rate: input.risk_free_rate,
        market_return: input.market_return,
        beta: input.beta,
        market_risk_premium,
        size_premium: input.size_premium,
        company_specific_premium: input.company_specific_premium,
        cost_of_equity,
    }
}

pub fn precedent_transaction(input: &PrecedentInput) -> PrecedentOutput {
    PrecedentOutput {
        transaction_price: input.transaction_price,
        target_ebitda: input.target_ebitda,
        target_revenue: input.target_revenue,
        implied_ev_ebitda: if input.target_ebitda > 0.0 {
            input.transaction_price / input.target_ebitda
        } else {
            0.0
        },
        implied_ev_revenue: if input.target_revenue > 0.0 {
            input.transaction_price / input.target_revenue
        } else {
            0.0
        },
        premium_paid: input.premium_paid,
        control_premium: input.premium_paid,
    }
}

pub fn book_value(input: &BookValueInput) -> BookValueOutput {
    BookValueOutput {
        total_assets: input.total_assets,
        total_liabilities: input.total_liabilities,
        intangible_assets: input.intangible_assets,
        tangible_book_value: input.total_assets - input.intangible_assets - input.total_liabilities,
        book_value_of_equity: input.total_assets
            - input.total_liabilities
            - input.preferred_equity
            - input.minority_interests,
        preferred_equity: input.preferred_equity,
        minority_interests: input.minority_interests,
    }
}

// Rounding helpers used by the endpoint layer where the service rounded.
pub fn rounded2(value: f64) -> f64 {
    r2(value)
}

pub fn rounded4(value: f64) -> f64 {
    r4(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn company() -> CompanyFinancials {
        CompanyFinancials {
            company_id: "C1".to_string(),
            company_name: "Acme".to_string(),
            fiscal_year_end: "2025-12-31".to_string(),
            revenue: 900.0,
            ebitda: 150.0,
            ebit: 120.0,
            net_income: 60.0,
            total_assets: 1200.0,
            total_equity: 700.0,
            total_debt: 200.0,
            cash: 80.0,
            shares_outstanding: 100.0,
            share_price: 8.0,
            beta: 1.1,
            dividend_per_share: 0.4,
        }
    }

    fn dcf_inputs() -> DcfInputs {
        DcfInputs {
            projection_years: 5,
            base_revenue: 900.0,
            revenue_cagr: 0.08,
            ebitda_margin: 0.2,
            depreciation_rate: 0.03,
            capex_rate: 0.04,
            tax_rate: 0.25,
            discount_rate: 0.1,
            terminal_growth_rate: 0.03,
            net_debt: 120.0,
        }
    }

    fn comparables() -> Vec<ComparableCompany> {
        vec![
            ComparableCompany {
                company_name: "X".to_string(),
                ticker: "X".to_string(),
                ev: 1000.0,
                ebitda: 100.0,
                revenue: 700.0,
                total_equity: 600.0,
                net_income: 50.0,
                shares_outstanding: 40.0,
            },
            // Zero EBITDA and zero earnings must drop out of both averages.
            ComparableCompany {
                company_name: "Y".to_string(),
                ticker: "Y".to_string(),
                ev: 800.0,
                ebitda: 0.0,
                revenue: 500.0,
                total_equity: 400.0,
                net_income: 0.0,
                shares_outstanding: 30.0,
            },
        ]
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn dcf_matches_the_reference_projection() {
        let out = dcf_valuation(&company(), &dcf_inputs());
        close(out.present_value_of_projections, 500.5987994073927);
        close(out.terminal_value, 2286.326963503544);
        close(out.present_value_of_terminal_value, 1419.6291631244408);
        close(out.enterprise_value, 1920.2279625318333);
        close(out.equity_value, 1800.2279625318333);
        close(out.value_per_share, 18.002279625318334);
        close(out.implied_ebitda_multiple, 10.667933125176852);
        close(out.implied_pe_ratio, 30.003799375530555);
    }

    #[test]
    fn implied_multiple_uses_the_dcf_base_revenue() {
        // The service read request.base_revenue, which does not exist, and
        // returned 500. The figure is derived from the DCF's own base now.
        let out = dcf_valuation(&company(), &dcf_inputs());
        assert!(out.implied_ebitda_multiple > 0.0);
        assert_eq!(
            out.implied_ebitda_multiple,
            out.enterprise_value / (900.0 * 0.2)
        );
    }

    #[test]
    fn comparables_average_only_positive_multiples() {
        let out = comparable_valuation(&company(), &comparables());
        assert_eq!(out.multiples.ev_ebitda, 10.0);
        assert_eq!(out.multiples.pe, 12.0);
        assert_eq!(out.adjusted_equity_value, 1010.0);
        assert_eq!(out.value_per_share, 10.1);
    }

    #[test]
    fn empty_comparable_set_uses_the_service_defaults() {
        let out = comparable_valuation(&company(), &[]);
        assert_eq!(out.multiples.ev_ebitda, 10.0);
        assert_eq!(out.multiples.pe, 15.0);
    }

    #[test]
    fn dividend_discount_is_gordon_growth() {
        let out = dividend_discount(&company());
        close(out.value_per_share, 6.000000000000001);
        close(out.valuation_vs_market, -24.99999999999999);
        assert_eq!(out.required_return, 0.12);
        assert_eq!(out.dividend_growth_rate, 0.05);
    }

    #[test]
    fn composite_without_dcf_matches_the_service() {
        let request = ValuationRequest {
            company: company(),
            include_dcf: false,
            include_comparables: true,
            include_ddm: true,
            dcf_inputs: Some(dcf_inputs()),
            comparables: comparables(),
        };
        let out = value_business(&request, "2026-11-03");
        assert!(out.dcf_valuation.is_none());
        close(out.weighted_average_value, 524.0);
        close(out.value_range_low, 510.00000000000006);
        close(out.value_range_high, 1161.4999999999998);
        assert_eq!(out.recommendation, "SELL");
        assert_eq!(out.valuation_date, "2026-11-03");
    }

    #[test]
    fn composite_with_dcf_now_completes_instead_of_crashing() {
        let request = ValuationRequest {
            company: company(),
            include_dcf: true,
            include_comparables: true,
            include_ddm: true,
            dcf_inputs: Some(dcf_inputs()),
            comparables: comparables(),
        };
        let out = value_business(&request, "2026-11-03");
        let dcf = out.dcf_valuation.expect("dcf valuation");
        close(dcf.value_per_share, 18.002279625318334);
        // weights are not renormalised: 0.4*18.002279... + 0.4*10.1 + 0.2*6.0
        close(
            out.weighted_average_value,
            (18.002279625318334 * 0.4 + 10.1 * 0.4 + 6.000000000000001 * 0.2) * 100.0,
        );
        assert_eq!(out.recommendation, "BUY");
    }

    #[test]
    fn no_method_results_yields_an_empty_range() {
        let request = ValuationRequest {
            company: company(),
            include_dcf: false,
            include_comparables: false,
            include_ddm: false,
            dcf_inputs: None,
            comparables: Vec::new(),
        };
        let out = value_business(&request, "2026-11-03");
        assert_eq!(out.value_range_low, 0.0);
        assert_eq!(out.value_range_high, 0.0);
        assert_eq!(out.recommendation, "SELL");
    }

    #[test]
    fn supporting_calculators_match_the_service() {
        let wacc = calculate_wacc(&WaccInput {
            market_cap: 1000.0,
            total_debt: 300.0,
            cost_of_equity: 0.11,
            cost_of_debt: 0.06,
            tax_rate: 0.25,
            debt_premium: 0.02,
        });
        assert_eq!(wacc.total_capital, 1300.0);
        close(wacc.equity_weight, 0.7692307692307693);
        close(wacc.after_tax_cost_of_debt, 0.045);
        close(wacc.wacc, 0.095);

        let capm = calculate_capm(&CapmInput {
            risk_free_rate: 0.03,
            market_return: 0.09,
            beta: 1.2,
            size_premium: 0.01,
            company_specific_premium: 0.0,
        });
        close(capm.market_risk_premium, 0.06);
        close(capm.cost_of_equity, 0.11199999999999999);

        let prec = precedent_transaction(&PrecedentInput {
            transaction_price: 500.0,
            target_ebitda: 50.0,
            target_revenue: 300.0,
            premium_paid: 0.25,
        });
        assert_eq!(prec.implied_ev_ebitda, 10.0);
        close(prec.implied_ev_revenue, 1.6666666666666667);
        assert_eq!(prec.control_premium, 0.25);

        let bv = book_value(&BookValueInput {
            total_assets: 1200.0,
            intangible_assets: 100.0,
            total_liabilities: 500.0,
            preferred_equity: 50.0,
            minority_interests: 20.0,
        });
        assert_eq!(bv.tangible_book_value, 600.0);
        assert_eq!(bv.book_value_of_equity, 630.0);
    }
}
