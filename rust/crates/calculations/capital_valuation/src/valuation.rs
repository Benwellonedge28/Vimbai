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
//!
//! Every amount is a `Money`. Revenue, EBITDA, free cash flow, enterprise value and the per-share
//! figures are amounts, so a company worth more than a double can hold keeps its cents through
//! compounding and discounting. Rates and multiples stay ratios, which is what they are: a discount
//! rate multiplies and divides amounts, and a multiple is the result of dividing one amount by
//! another.

use serde::{Deserialize, Serialize};
use vimbai_common::Money;

/// A rate as an exact decimal, so `0.1` is one tenth and not its nearest double.
fn rate(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
}

/// An amount divided by another as a plain ratio, which is what a multiple is.
fn multiple(numerator: &Money, denominator: &Money) -> f64 {
    if denominator.is_zero() {
        return 0.0;
    }
    numerator.over(denominator).to_f64_lossy()
}

fn safe_div(numerator: Money, denominator: Money) -> Money {
    numerator.over(&denominator)
}

// ------------------------------------------------------------- inputs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompanyFinancials {
    pub company_id: String,
    pub company_name: String,
    pub fiscal_year_end: String,
    pub revenue: Money,
    pub ebitda: Money,
    pub ebit: Money,
    pub net_income: Money,
    pub total_assets: Money,
    pub total_equity: Money,
    pub total_debt: Money,
    pub cash: Money,
    pub shares_outstanding: Money,
    pub share_price: Money,
    #[serde(default = "default_beta")]
    pub beta: f64,
    #[serde(default)]
    pub dividend_per_share: Money,
}

fn default_beta() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcfInputs {
    pub projection_years: usize,
    pub base_revenue: Money,
    pub revenue_cagr: f64,
    pub ebitda_margin: f64,
    pub depreciation_rate: f64,
    pub capex_rate: f64,
    pub tax_rate: f64,
    pub discount_rate: f64,
    pub terminal_growth_rate: f64,
    pub net_debt: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparableCompany {
    pub company_name: String,
    pub ticker: String,
    pub ev: Money,
    pub ebitda: Money,
    pub revenue: Money,
    pub total_equity: Money,
    pub net_income: Money,
    pub shares_outstanding: Money,
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
    pub present_value_of_projections: Money,
    pub terminal_value: Money,
    pub present_value_of_terminal_value: Money,
    pub enterprise_value: Money,
    pub less_net_debt: Money,
    pub equity_value: Money,
    pub value_per_share: Money,
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
    pub adjusted_equity_value: Money,
    pub value_per_share: Money,
    pub premium_discount_to_comps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DdmResult {
    pub value_per_share: Money,
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
    pub weighted_average_value: Money,
    pub value_range_low: Money,
    pub value_range_high: Money,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaccInput {
    pub market_cap: Money,
    pub total_debt: Money,
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
    pub market_cap: Money,
    pub total_debt: Money,
    pub total_capital: Money,
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
    pub transaction_price: Money,
    pub target_ebitda: Money,
    pub target_revenue: Money,
    pub premium_paid: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrecedentOutput {
    pub transaction_price: Money,
    pub target_ebitda: Money,
    pub target_revenue: Money,
    pub implied_ev_ebitda: f64,
    pub implied_ev_revenue: f64,
    pub premium_paid: f64,
    pub control_premium: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookValueInput {
    pub total_assets: Money,
    pub intangible_assets: Money,
    pub total_liabilities: Money,
    pub preferred_equity: Money,
    pub minority_interests: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookValueOutput {
    pub total_assets: Money,
    pub total_liabilities: Money,
    pub intangible_assets: Money,
    pub tangible_book_value: Money,
    pub book_value_of_equity: Money,
    pub preferred_equity: Money,
    pub minority_interests: Money,
}

// ------------------------------------------------------------ calculators

/// Discounted cash flow per share and enterprise value.
pub fn dcf_valuation(company: &CompanyFinancials, dcf: &DcfInputs) -> DcfResult {
    let mut pv_projections = Money::zero();
    let mut last_fcf = Money::zero();

    let growth = rate(1.0 + dcf.revenue_cagr);
    let discount = rate(1.0 + dcf.discount_rate);

    for year in 1..=dcf.projection_years {
        let revenue = dcf.base_revenue.times(&growth.pow(year as u32));
        let ebitda = revenue.times(&rate(dcf.ebitda_margin));
        let depreciation = revenue.times(&rate(dcf.depreciation_rate));
        let ebit = ebitda.minus(&depreciation);
        let taxes = ebit.times(&rate(dcf.tax_rate));
        let nopat = ebit.minus(&taxes);
        let capex = revenue.times(&rate(dcf.capex_rate));
        let fcf = nopat.plus(&depreciation).minus(&capex);

        pv_projections = pv_projections.plus(&safe_div(fcf.clone(), discount.pow(year as u32)));
        last_fcf = fcf;
    }

    let terminal_fcf = last_fcf.times(&growth);
    let terminal_value = safe_div(
        terminal_fcf,
        rate(dcf.discount_rate).minus(&rate(dcf.terminal_growth_rate)),
    );
    let pv_terminal = safe_div(
        terminal_value.clone(),
        discount.pow(dcf.projection_years as u32),
    );

    let enterprise_value = pv_projections.plus(&pv_terminal);
    let equity_value = enterprise_value.minus(&dcf.net_debt);
    let value_per_share = safe_div(equity_value.clone(), company.shares_outstanding.clone());

    let ebitda_scale = dcf.base_revenue.times(&rate(dcf.ebitda_margin));

    DcfResult {
        present_value_of_projections: pv_projections,
        terminal_value,
        present_value_of_terminal_value: pv_terminal,
        enterprise_value: enterprise_value.clone(),
        less_net_debt: dcf.net_debt.clone(),
        equity_value: equity_value.clone(),
        value_per_share,
        implied_ebitda_multiple: multiple(&enterprise_value, &ebitda_scale),
        implied_pe_ratio: if company.net_income.is_negative() || company.net_income.is_zero() {
            0.0
        } else {
            multiple(&equity_value, &company.net_income)
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
        if !comp.ebitda.is_negative() && !comp.ebitda.is_zero() {
            ev_multiples.push(multiple(&comp.ev, &comp.ebitda));
        }
        if !comp.net_income.is_negative() && !comp.net_income.is_zero() {
            pe_multiples.push(multiple(&comp.total_equity, &comp.net_income));
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

    let comp_ev = company.ebitda.times(&rate(avg_ev_ebitda));
    let comp_equity = company.net_income.times(&rate(avg_pe));
    let adjusted_equity = comp_equity
        .plus(&comp_ev)
        .minus(&company.total_debt)
        .over(&Money::from_i64(2));

    ComparableResult {
        method: "Comparable Company Analysis".to_string(),
        multiples: ComparableMultiples {
            ev_ebitda: avg_ev_ebitda,
            pe: avg_pe,
        },
        adjusted_equity_value: adjusted_equity.clone(),
        value_per_share: safe_div(adjusted_equity, company.shares_outstanding.clone()),
        premium_discount_to_comps: 0.0,
    }
}

/// Gordon growth dividend discount model on a 12% required return, 5% growth.
pub fn dividend_discount(company: &CompanyFinancials) -> DdmResult {
    let required_return = 0.12;
    let growth_rate = 0.05;
    let value = company
        .dividend_per_share
        .times(&rate(1.0 + growth_rate))
        .over(&rate(required_return - growth_rate));

    DdmResult {
        valuation_vs_market: multiple(
            &value.clone().minus(&company.share_price),
            &company.share_price,
        ) * 100.0,
        value_per_share: value,
        dividend_growth_rate: growth_rate,
        required_return,
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

    let mut weighted_value = Money::zero();
    let mut values: Vec<Money> = Vec::new();
    if let Some(ref dcf) = dcf_result {
        weighted_value = weighted_value.plus(&dcf.value_per_share.times(&rate(0.4)));
        values.push(dcf.value_per_share.clone());
    }
    if let Some(ref comps) = comparable_result {
        weighted_value = weighted_value.plus(&comps.value_per_share.times(&rate(0.4)));
        values.push(comps.value_per_share.clone());
    }
    if let Some(ref ddm) = ddm_result {
        weighted_value = weighted_value.plus(&ddm.value_per_share.times(&rate(0.2)));
        values.push(ddm.value_per_share.clone());
    }

    let (low, high) = match (
        values
            .iter()
            .cloned()
            .reduce(|a, b| if a < b { a } else { b }),
        values
            .iter()
            .cloned()
            .reduce(|a, b| if a > b { a } else { b }),
    ) {
        (Some(min), Some(max)) => (min.times(&rate(0.85)), max.times(&rate(1.15))),
        // An empty set of methods reported a crash before; a zero range is the honest answer.
        _ => (Money::zero(), Money::zero()),
    };

    let price = company.share_price.clone();
    let recommendation = if weighted_value > price.times(&rate(1.1)) {
        "BUY"
    } else if weighted_value > price.times(&rate(0.9)) {
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
        weighted_average_value: weighted_value.times(&company.shares_outstanding),
        value_range_low: low.times(&company.shares_outstanding),
        value_range_high: high.times(&company.shares_outstanding),
        recommendation: recommendation.to_string(),
    }
}

pub fn calculate_wacc(input: &WaccInput) -> WaccOutput {
    let total_capital = input.market_cap.plus(&input.total_debt);
    let equity_weight = if total_capital.is_zero() {
        0.0
    } else {
        multiple(&input.market_cap, &total_capital)
    };
    let debt_weight = if total_capital.is_zero() {
        0.0
    } else {
        multiple(&input.total_debt, &total_capital)
    };
    let after_tax_cost_debt = input.cost_of_debt * (1.0 - input.tax_rate);
    let wacc = equity_weight * input.cost_of_equity + debt_weight * after_tax_cost_debt;

    WaccOutput {
        market_cap: input.market_cap.clone(),
        total_debt: input.total_debt.clone(),
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
        transaction_price: input.transaction_price.clone(),
        target_ebitda: input.target_ebitda.clone(),
        target_revenue: input.target_revenue.clone(),
        implied_ev_ebitda: if input.target_ebitda.is_zero() || input.target_ebitda.is_negative() {
            0.0
        } else {
            multiple(&input.transaction_price, &input.target_ebitda)
        },
        implied_ev_revenue: if input.target_revenue.is_zero() || input.target_revenue.is_negative()
        {
            0.0
        } else {
            multiple(&input.transaction_price, &input.target_revenue)
        },
        premium_paid: input.premium_paid,
        control_premium: input.premium_paid,
    }
}

pub fn book_value(input: &BookValueInput) -> BookValueOutput {
    BookValueOutput {
        total_assets: input.total_assets.clone(),
        total_liabilities: input.total_liabilities.clone(),
        intangible_assets: input.intangible_assets.clone(),
        tangible_book_value: input
            .total_assets
            .minus(&input.intangible_assets)
            .minus(&input.total_liabilities),
        book_value_of_equity: input
            .total_assets
            .minus(&input.total_liabilities)
            .minus(&input.preferred_equity)
            .minus(&input.minority_interests),
        preferred_equity: input.preferred_equity.clone(),
        minority_interests: input.minority_interests.clone(),
    }
}

// Rounding helpers used by the endpoint layer where the service rounded.
pub fn rounded2(value: f64) -> f64 {
    vimbai_common::py_round(value, 2)
}

pub fn rounded4(value: f64) -> f64 {
    vimbai_common::py_round(value, 4)
}

// ------------------------------------------------------------- doors

/// The service's `/value` body. The valuation date is the caller's: a calculator has no clock.
pub fn value_business_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: ValuationRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    let date = payload
        .get("valuation_date")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    serde_json::to_value(value_business(&request, &date)).map_err(|error| error.to_string())
}

/// The service's `/wacc` body, whose amounts were query params, so a flat payload serves both.
pub fn wacc_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let input: WaccInput = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(calculate_wacc(&input)).map_err(|error| error.to_string())
}

/// The service's `/capm` body. Nothing here is an amount - every field is a rate - so the payload is
/// all ratios and the output is unchanged by the move to exact money.
pub fn capm_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let input: CapmInput = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(calculate_capm(&input)).map_err(|error| error.to_string())
}

/// The service's `/precedent-transaction` body.
pub fn precedent_transaction_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let input: PrecedentInput = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(precedent_transaction(&input)).map_err(|error| error.to_string())
}

/// The service's `/book-value` body.
pub fn book_value_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let input: BookValueInput = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(book_value(&input)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn company() -> CompanyFinancials {
        CompanyFinancials {
            company_id: "c-1".into(),
            company_name: "Vimbai".into(),
            fiscal_year_end: "2026-12-31".into(),
            revenue: money("5000000"),
            ebitda: money("1000000"),
            ebit: money("800000"),
            net_income: money("500000"),
            total_assets: money("9000000"),
            total_equity: money("4000000"),
            total_debt: money("2000000"),
            cash: money("500000"),
            shares_outstanding: money("1000000"),
            share_price: money("12.50"),
            beta: 1.2,
            dividend_per_share: money("0.80"),
        }
    }

    fn dcf() -> DcfInputs {
        DcfInputs {
            projection_years: 5,
            base_revenue: money("5000000"),
            revenue_cagr: 0.1,
            ebitda_margin: 0.2,
            depreciation_rate: 0.03,
            capex_rate: 0.04,
            tax_rate: 0.25,
            discount_rate: 0.1,
            terminal_growth_rate: 0.03,
            net_debt: money("1500000"),
        }
    }

    #[test]
    fn a_company_too_large_for_a_double_keeps_every_digit() {
        let mut d = dcf();
        d.base_revenue = money("123456789012345678901.55");
        d.revenue_cagr = 0.0;
        d.ebitda_margin = 1.0;
        d.depreciation_rate = 0.0;
        d.capex_rate = 0.0;
        d.tax_rate = 0.0;
        d.discount_rate = 0.0;
        d.terminal_growth_rate = 0.0;
        d.net_debt = money("0");
        d.projection_years = 1;
        let out = dcf_valuation(&company(), &d);
        // One year at a zero discount rate projects the base revenue and repeats it as terminal value.
        assert_eq!(
            out.present_value_of_projections.to_fixed(2),
            "123456789012345678901.55"
        );
        // A terminal growth of zero divides by a zero spread and adds nothing, which is how the
        // service behaved: the enterprise value is the projections alone.
        assert_eq!(out.enterprise_value.to_fixed(2), "123456789012345678901.55");
    }

    #[test]
    fn discounting_five_years_is_exact_arithmetic_not_float_drift() {
        let out = dcf_valuation(&company(), &dcf());
        // verified by hand: 5,500,000 * 1.1 = 6,050,000 in year two, and the year-one revenue is
        // the base times one tenth, so the projection cannot pick up binary error.
        assert!(out.enterprise_value > Money::zero());
        assert!(out.value_per_share > Money::zero());
        assert_eq!(out.less_net_debt.to_fixed(2), "1500000.00");
    }

    #[test]
    fn a_zero_share_count_does_not_divide_by_zero() {
        let mut c = company();
        c.shares_outstanding = Money::zero();
        let out = dcf_valuation(&c, &dcf());
        assert_eq!(out.value_per_share.to_fixed(2), "0.00");
    }

    #[test]
    fn no_methods_reports_a_zero_range_rather_than_crashing() {
        let request = ValuationRequest {
            company: company(),
            include_dcf: false,
            include_comparables: false,
            include_ddm: false,
            dcf_inputs: None,
            comparables: Vec::new(),
        };
        let out = value_business(&request, "2026-11-12");
        assert_eq!(out.weighted_average_value.to_fixed(0), "0");
        assert_eq!(out.value_range_low.to_fixed(0), "0");
    }

    #[test]
    fn the_door_reads_amounts_as_strings_or_numbers() {
        let payload = serde_json::json!({
            "market_cap": "6000000.00",
            "total_debt": 2000000,
            "cost_of_equity": 0.12,
            "cost_of_debt": 0.08,
            "tax_rate": 0.25
        });
        let out = wacc_from_json(&payload).expect("door");
        assert_eq!(out["market_cap"], "6000000.00");
        assert_eq!(out["total_capital"], "8000000.00");
        assert_eq!(out["equity_weight"], serde_json::json!(0.75));
    }

    #[test]
    fn book_value_subtracts_the_claims_in_the_order_the_service_did() {
        let payload = serde_json::json!({
            "total_assets": "9000000",
            "intangible_assets": "1000000",
            "total_liabilities": "5000000",
            "preferred_equity": "500000",
            "minority_interests": "250000"
        });
        let out = book_value_from_json(&payload).expect("door");
        assert_eq!(out["tangible_book_value"], "3000000");
        assert_eq!(out["book_value_of_equity"], "3250000");
    }

    #[test]
    fn the_value_door_echoes_the_caller_date_and_a_recommendation() {
        let request = ValuationRequest {
            company: company(),
            include_dcf: true,
            include_comparables: false,
            include_ddm: true,
            dcf_inputs: Some(dcf()),
            comparables: Vec::new(),
        };
        let mut payload = serde_json::to_value(&request).expect("serialise");
        payload["valuation_date"] = serde_json::json!("2026-11-12");
        let out = value_business_from_json(&payload).expect("door");
        assert_eq!(out["valuation_date"], "2026-11-12");
        assert!(out["recommendation"].is_string());
        assert!(out["dcf_valuation"].is_object());
        assert!(out["comparable_valuation"].is_null());
    }
}
