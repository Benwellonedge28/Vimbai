"""
Economic Value Added Service
Port: 8212
EVA calculation and wealth creation analysis
"""

from typing import Any, Dict

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Economic Value Added Service", version="1.0.0")


class EVAMetrics(BaseModel):
    invested_capital: float
    weighted_average_cost_of_capital: float
    net_operating_profit_after_tax: float
    capital_charge: float
    economic_value_added: float
    value_creation_index: float


class EVARequest(BaseModel):
    company_id: str
    period: str
    total_assets: float
    current_liabilities: float
    equity: float
    debt: float
    cost_of_debt: float
    cost_of_equity: float
    ebit: float
    tax_rate: float
    expected_growth: float


class EVAResponse(BaseModel):
    company_id: str
    period: str
    eva_metrics: EVAMetrics
    eva_by_segment: Dict[str, float]
    eva_momentum: str
    wealth_created: bool
    recommendations: list


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
    return {"status": "healthy", "service": "eva", "version": "1.0.0"}


@app.post("/calculate", response_model=EVAResponse)
async def calculate_eva(request: EVARequest):
    logger.info("Calculating EVA", company=request.company_id, period=request.period)

    debt_ratio = request.debt / (request.debt + request.equity) if (request.debt + request.equity) else 0
    equity_ratio = request.equity / (request.debt + request.equity) if (request.debt + request.equity) else 0

    wacc = debt_ratio * request.cost_of_debt * (1 - request.tax_rate) + equity_ratio * request.cost_of_equity

    invested_capital = request.total_assets - request.current_liabilities

    nopat = request.ebit * (1 - request.tax_rate)

    capital_charge = invested_capital * wacc

    eva = nopat - capital_charge

    value_creation_index = (nopat / capital_charge) if capital_charge else 0

    wealth_created = eva > 0

    eva_momentum = "improving" if eva > 0 else "declining" if eva < -1000000 else "stable"

    return EVAResponse(
        company_id=request.company_id,
        period=request.period,
        eva_metrics=EVAMetrics(
            invested_capital=round(invested_capital, 2),
            weighted_average_cost_of_capital=round(wacc, 4),
            net_operating_profit_after_tax=round(nopat, 2),
            capital_charge=round(capital_charge, 2),
            economic_value_added=round(eva, 2),
            value_creation_index=round(value_creation_index, 2),
        ),
        eva_by_segment={
            "core_operations": round(eva * 0.7, 2),
            "growth_investments": round(eva * 0.2, 2),
            "non_operating": round(eva * 0.1, 2),
        },
        eva_momentum=eva_momentum,
        wealth_created=wealth_created,
        recommendations=(
            [
                "Focus on capital efficiency to improve EVA",
                "Review underperforming business segments",
                "Optimize working capital management",
            ]
            if not wealth_created
            else [
                "Continue value-creating investments",
                "Monitor WACC changes",
                "Consider share buybacks if excess capital",
            ]
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8212)
