"""
Cash Return on Invested Capital Service
Port: 8214
CROIC calculation and capital efficiency
"""

import importlib.util
import os
import sys
from typing import Any, Dict

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("croic_service_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["croic_service_calc"] = calc
_spec.loader.exec_module(calc)

app = FastAPI(title="CROIC Service", version="1.0.0")


class CROICMetrics(BaseModel):
    free_cash_flow: float
    invested_capital: float
    cash_return_on_invested_capital: float
    croic_5_year_avg: float
    capital_efficiency_rating: str


class CROICRequest(BaseModel):
    company_id: str
    periods: list
    operating_cash_flow: float
    capital_expenditures: float
    total_assets: float
    current_liabilities: float
    tax_rate: float


class CROICResponse(BaseModel):
    company_id: str
    current_croic: CROICMetrics
    historical_croic: list
    industry_benchmark: float
    croic_vs_industry: str
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
    return {
        "status": "healthy",
        "service": "croic",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    """The original Python calculation, kept as the oracle."""
    fcf = payload["operating_cash_flow"] - payload["capital_expenditures"]

    invested_capital = payload["total_assets"] - payload["current_liabilities"]

    croic = (fcf / invested_capital) if invested_capital else 0

    capital_efficiency = "excellent" if croic > 0.15 else "good" if croic > 0.10 else "fair" if croic > 0.05 else "poor"

    historical = [{"period": p, "croic": round(croic * 0.9, 4)} for p in payload["periods"]]
    avg_5yr = sum(h["croic"] for h in historical) / len(historical) if historical else croic

    industry_avg = 0.12
    vs_industry = "outperforming" if croic > industry_avg else "underperforming"

    return dict(
        company_id=payload["company_id"],
        current_croic=CROICMetrics(
            free_cash_flow=round(fcf, 2),
            invested_capital=round(invested_capital, 2),
            cash_return_on_invested_capital=round(croic, 4),
            croic_5_year_avg=round(avg_5yr, 4),
            capital_efficiency_rating=capital_efficiency,
        ),
        historical_croic=historical,
        industry_benchmark=industry_avg,
        croic_vs_industry=vs_industry,
        recommendations=(
            [
                "Maintain high CROIC through disciplined capital allocation",
                "Reinvest in high-return projects",
                "Return excess cash to shareholders if limited opportunities",
            ]
            if capital_efficiency in ["excellent", "good"]
            else [
                "Improve operational efficiency to boost FCF",
                "Review capital structure",
                "Divest underperforming assets",
            ]
        ),
    )


@app.post("/calculate", response_model=CROICResponse)
async def croic_calculation(request: CROICRequest):
    logger.info("Calculating", company=request.company_id)
    result = calc.run(request.model_dump(), _reference)
    return CROICResponse(**result) if isinstance(result, dict) else result


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8214)
