"""
DuPont Analysis Service
Port: 8211
ROE decomposition analysis
"""

from typing import Any, Dict

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="DuPont Analysis Service", version="1.0.0")


class DuPontMetrics(BaseModel):
    net_profit_margin: float
    asset_turnover: float
    financial_leverage: float
    tax_burden: float
    interest_burden: float
    return_on_equity: float


class DuPontRequest(BaseModel):
    company_id: str
    period: str
    revenue: float
    net_income: float
    ebt: float
    ebit: float
    total_assets: float
    total_equity: float
    total_debt: float


class DuPontResponse(BaseModel):
    company_id: str
    period: str
    du_pont_metrics: DuPontMetrics
    three_factor_breakdown: Dict[str, float]
    five_factor_breakdown: Dict[str, float]
    roe_contribution: Dict[str, float]
    peer_comparison: Dict[str, float]
    conclusions: Dict[str, str]


async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "du-pont", "version": "1.0.0"}


@app.post("/analyze", response_model=DuPontResponse)
async def perform_du_pont_analysis(request: DuPontRequest):
    logger.info("Performing DuPont analysis", company=request.company_id, period=request.period)

    net_profit_margin = request.net_income / request.revenue if request.revenue else 0
    asset_turnover = request.revenue / request.total_assets if request.total_assets else 0
    financial_leverage = request.total_assets / request.total_equity if request.total_equity else 0
    tax_burden = request.net_income / request.ebt if request.ebt else 0
    interest_burden = request.ebt / request.ebit if request.ebit else 0

    three_factor_roe = net_profit_margin * asset_turnover * financial_leverage

    five_factor_roe = (
        tax_burden
        * interest_burden
        * (request.ebit / request.revenue if request.revenue else 0)
        * (request.revenue / request.total_assets if request.total_assets else 0)
        * (request.total_assets / request.total_equity if request.total_equity else 0)
    )

    roe_contribution = {
        "profit_margin_contribution": round(net_profit_margin * 100, 2),
        "asset_turnover_contribution": round(asset_turnover * 100, 2),
        "financial_leverage_contribution": round(financial_leverage * 100, 2),
        "tax_efficiency": round(tax_burden * 100, 2),
        "interest_impact": round(interest_burden * 100, 2),
    }

    peer_benchmark = {
        "industry_avg_roe": 15.0,
        "company_roe": round(three_factor_roe * 100, 2),
        "vs_industry": round(three_factor_roe * 100 - 15.0, 2),
    }

    conclusions = {
        "profitability": "Strong" if net_profit_margin > 0.1 else "Moderate" if net_profit_margin > 0.05 else "Weak",
        "efficiency": "High" if asset_turnover > 1.5 else "Moderate" if asset_turnover > 1.0 else "Low",
        "leverage": (
            "Conservative" if financial_leverage < 2.0 else "Moderate" if financial_leverage < 3.0 else "Aggressive"
        ),
    }

    return DuPontResponse(
        company_id=request.company_id,
        period=request.period,
        du_pont_metrics=DuPontMetrics(
            net_profit_margin=round(net_profit_margin, 4),
            asset_turnover=round(asset_turnover, 4),
            financial_leverage=round(financial_leverage, 4),
            tax_burden=round(tax_burden, 4),
            interest_burden=round(interest_burden, 4),
            return_on_equity=round(three_factor_roe, 4),
        ),
        three_factor_breakdown={
            "net_profit_margin": round(net_profit_margin, 4),
            "asset_turnover": round(asset_turnover, 4),
            "financial_leverage": round(financial_leverage, 4),
            "calculated_roe": round(three_factor_roe, 4),
        },
        five_factor_breakdown={
            "tax_burden": round(tax_burden, 4),
            "interest_burden": round(interest_burden, 4),
            "operating_margin": round(request.ebit / request.revenue, 4) if request.revenue else 0,
            "asset_turnover": round(asset_turnover, 4),
            "financial_leverage": round(financial_leverage, 4),
            "calculated_roe": round(five_factor_roe, 4),
        },
        roe_contribution=roe_contribution,
        peer_comparison=peer_benchmark,
        conclusions=conclusions,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8211)
