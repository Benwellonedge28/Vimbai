"""
Business Valuation Service
Port: 8155
DCF valuation, comparable company analysis, dividend discount model
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

logger = structlog.get_logger()
app = FastAPI(title="Business Valuation Service", version="1.0.0")


# Pydantic Models
class CompanyFinancials(BaseModel):
    company_id: str
    company_name: str
    fiscal_year_end: str
    revenue: float
    ebitda: float
    ebit: float
    net_income: float
    total_assets: float
    total_equity: float
    total_debt: float
    cash: float
    shares_outstanding: int
    share_price: float
    beta: float = 1.0
    dividend_per_share: float = 0.0


class DCFInputs(BaseModel):
    projection_years: int = Field(default=5, ge=3, le=10)
    base_revenue: float
    revenue_cagr: float = Field(ge=-0.2, le=0.5)
    ebitda_margin: float
    depreciation_rate: float
    capex_rate: float
    tax_rate: float
    discount_rate: float
    terminal_growth_rate: float
    net_debt: float


class ComparableCompany(BaseModel):
    company_name: str
    ticker: str
    ev: float
    ebitda: float
    revenue: float
    total_equity: float
    net_income: float
    shares_outstanding: int


class ValuationRequest(BaseModel):
    company: CompanyFinancials
    include_dcf: bool = True
    include_comparables: bool = True
    include_ddm: bool = True
    dcf_inputs: Optional[DCFInputs] = None
    comparables: List[ComparableCompany] = []


class DCFResult(BaseModel):
    present_value_of_projections: float
    terminal_value: float
    present_value_of_terminal_value: float
    enterprise_value: float
    less_net_debt: float
    equity_value: float
    value_per_share: float
    implied_ebitda_multiple: float
    implied_pe_ratio: float


class ComparableResult(BaseModel):
    method: str
    multiples: Dict[str, float]
    adjusted_equity_value: float
    value_per_share: float
    premium_discount_to_comps: float


class DDMResult(BaseModel):
    value_per_share: float
    dividend_growth_rate: float
    required_return: float
    valuation_vs_market: float


class ValuationResponse(BaseModel):
    company_id: str
    valuation_date: str
    dcf_valuation: Optional[DCFResult] = None
    comparable_valuation: Optional[ComparableResult] = None
    dividend_discount_valuation: Optional[DDMResult] = None
    weighted_average_value: float
    value_range_low: float
    value_range_high: float
    recommendation: str


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal Vimbai service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "business-valuation", "version": "1.0.0"}


@app.post("/value", response_model=ValuationResponse)
async def value_company(request: ValuationRequest):
    """Perform comprehensive business valuation."""
    logger.info("Valuing company", company=request.company.company_name)

    dcf_result = None
    comparable_result = None
    ddm_result = None

    # DCF Valuation
    if request.include_dcf and request.dcf_inputs:
        dcf = request.dcf_inputs
        pv_projections = 0.0
        revenues = []
        free_cash_flows = []

        for year in range(1, dcf.projection_years + 1):
            revenue = dcf.base_revenue * ((1 + dcf.revenue_cagr) ** year)
            ebitda = revenue * dcf.ebitda_margin
            depreciation = revenue * dcf.depreciation_rate
            ebit = ebitda - depreciation
            taxes = ebit * dcf.tax_rate
            nopat = ebit - taxes
            capex = revenue * dcf.capex_rate
            fcf = nopat + depreciation - capex

            pv = fcf / ((1 + dcf.discount_rate) ** year)
            pv_projections += pv
            free_cash_flows.append({"year": year, "fcf": fcf, "pv": pv})
            revenues.append({"year": year, "revenue": revenue})

        # Terminal value
        terminal_fcf = free_cash_flows[-1]["fcf"] * (1 + dcf.terminal_growth_rate)
        terminal_value = terminal_fcf / (dcf.discount_rate - dcf.terminal_growth_rate)
        pv_terminal = terminal_value / ((1 + dcf.discount_rate) ** dcf.projection_years)

        ev = pv_projections + pv_terminal
        equity_value = ev - dcf.net_debt
        value_per_share = equity_value / request.company.shares_outstanding

        dcf_result = DCFResult(
            present_value_of_projections=pv_projections,
            terminal_value=terminal_value,
            present_value_of_terminal_value=pv_terminal,
            enterprise_value=ev,
            less_net_debt=dcf.net_debt,
            equity_value=equity_value,
            value_per_share=value_per_share,
            implied_ebitda_multiple=(
                ev / (request.base_revenue * dcf.ebitda_margin) if request.base_revenue * dcf.ebitda_margin > 0 else 0
            ),
            implied_pe_ratio=equity_value / request.company.net_income if request.company.net_income > 0 else 0,
        )

    # Comparable Company Analysis
    if request.include_comparables and request.comparables:
        ev_multiples = []
        ebitda_multiples = []
        pe_multiples = []

        for comp in request.comparables:
            if comp.ebitda > 0:
                ev_multiples.append(comp.ev / comp.ebitda)
            if comp.net_income > 0:
                pe_multiples.append(comp.total_equity / comp.net_income)

        avg_ev_ebitda = sum(ev_multiples) / len(ev_multiples) if ev_multiples else 10.0
        avg_pe = sum(pe_multiples) / len(pe_multiples) if pe_multiples else 15.0

        comp_ev = request.company.ebitda * avg_ev_ebitda
        comp_equity = request.company.net_income * avg_pe
        adjusted_equity = (comp_equity + comp_ev - request.company.total_debt) / 2
        comp_value_per_share = adjusted_equity / request.company.shares_outstanding

        comparable_result = ComparableResult(
            method="Comparable Company Analysis",
            multiples={"EV/EBITDA": avg_ev_ebitda, "P/E": avg_pe},
            adjusted_equity_value=adjusted_equity,
            value_per_share=comp_value_per_share,
            premium_discount_to_comps=0.0,
        )

    # Dividend Discount Model
    if request.include_ddm:
        dps = request.company.dividend_per_share
        price = request.company.share_price
        required_return = 0.12  # CAPM or WACC
        growth_rate = 0.05  # Assumed sustainable growth

        # Gordon Growth Model
        ddm_value = dps * (1 + growth_rate) / (required_return - growth_rate)

        ddm_result = DDMResult(
            value_per_share=ddm_value,
            dividend_growth_rate=growth_rate,
            required_return=required_return,
            valuation_vs_market=(ddm_value - price) / price * 100,
        )

    # Weighted average
    weights = {"dcf": 0.4, "comparables": 0.4, "ddm": 0.2}
    weighted_value = 0.0
    values = []
    if dcf_result:
        weighted_value += dcf_result.value_per_share * weights["dcf"]
        values.append(dcf_result.value_per_share)
    if comparable_result:
        weighted_value += comparable_result.value_per_share * weights["comparables"]
        values.append(comparable_result.value_per_share)
    if ddm_result:
        weighted_value += ddm_result.value_per_share * weights["ddm"]
        values.append(ddm_result.value_per_share)

    low_value = min(values) * 0.85
    high_value = max(values) * 1.15

    recommendation = (
        "BUY"
        if weighted_value > request.company.share_price * 1.1
        else "HOLD" if weighted_value > request.company.share_price * 0.9 else "SELL"
    )

    response = ValuationResponse(
        company_id=request.company.company_id,
        valuation_date=datetime.now().date().isoformat(),
        dcf_valuation=dcf_result,
        comparable_valuation=comparable_result,
        dividend_discount_valuation=ddm_result,
        weighted_average_value=weighted_value * request.company.shares_outstanding,
        value_range_low=low_value * request.company.shares_outstanding,
        value_range_high=high_value * request.company.shares_outstanding,
        recommendation=recommendation,
    )

    logger.info("Valuation complete", company=request.company.company_name, value=weighted_value)
    return response


@app.post("/wacc")
async def calculate_wacc(
    market_cap: float,
    total_debt: float,
    cost_of_equity: float,
    cost_of_debt: float,
    tax_rate: float,
    debt_premium: float = 0.02,
):
    """Calculate Weighted Average Cost of Capital."""
    total_capital = market_cap + total_debt
    equity_weight = market_cap / total_capital
    debt_weight = total_debt / total_capital

    after_tax_cost_debt = cost_of_debt * (1 - tax_rate)
    wacc = equity_weight * cost_of_equity + debt_weight * after_tax_cost_debt

    return {
        "market_cap": market_cap,
        "total_debt": total_debt,
        "total_capital": total_capital,
        "equity_weight": equity_weight,
        "debt_weight": debt_weight,
        "cost_of_equity": cost_of_equity,
        "cost_of_debt": cost_of_debt,
        "after_tax_cost_of_debt": after_tax_cost_debt,
        "wacc": wacc,
    }


@app.post("/capm")
async def calculate_capm(
    risk_free_rate: float,
    market_return: float,
    beta: float,
    size_premium: float = 0.0,
    company_specific_premium: float = 0.0,
):
    """Calculate cost of equity using CAPM."""
    market_risk_premium = market_return - risk_free_rate
    cost_of_equity = risk_free_rate + beta * market_risk_premium + size_premium + company_specific_premium

    return {
        "risk_free_rate": risk_free_rate,
        "market_return": market_return,
        "beta": beta,
        "market_risk_premium": market_risk_premium,
        "size_premium": size_premium,
        "company_specific_premium": company_specific_premium,
        "cost_of_equity": cost_of_equity,
    }


@app.post("/precedent-transaction")
async def calculate_precedent_valuation(
    transaction_price: float, target_ebitda: float, target_revenue: float, premium_paid: float
):
    """Calculate valuation based on precedent transactions."""
    implied_ev_ebitda = transaction_price / target_ebitda if target_ebitda > 0 else 0
    implied_ev_revenue = transaction_price / target_revenue if target_revenue > 0 else 0

    return {
        "transaction_price": transaction_price,
        "target_ebitda": target_ebitda,
        "target_revenue": target_revenue,
        "implied_ev_ebitda": implied_ev_ebitda,
        "implied_ev_revenue": implied_ev_revenue,
        "premium_paid": premium_paid,
        "control_premium": premium_paid,
    }


@app.post("/book-value")
async def calculate_book_value(
    total_assets: float,
    intangible_assets: float,
    total_liabilities: float,
    preferred_equity: float,
    minority_interests: float,
):
    """Calculate book value of equity."""
    tangible_book_value = total_assets - intangible_assets - total_liabilities
    book_value = total_assets - total_liabilities - preferred_equity - minority_interests

    return {
        "total_assets": total_assets,
        "total_liabilities": total_liabilities,
        "intangible_assets": intangible_assets,
        "tangible_book_value": tangible_book_value,
        "book_value_of_equity": book_value,
        "preferred_equity": preferred_equity,
        "minority_interests": minority_interests,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8155)
