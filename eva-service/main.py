"""
Economic Value Added Service
Port: 8212
EVA calculation and wealth creation analysis
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
_spec = importlib.util.spec_from_file_location("eva_service_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["eva_service_calc"] = calc
_spec.loader.exec_module(calc)

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
    return {
        "status": "healthy",
        "service": "eva",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    """The original Python calculation, kept as the oracle."""
    debt_ratio = payload["debt"] / (payload["debt"] + payload["equity"]) if (payload["debt"] + payload["equity"]) else 0
    equity_ratio = (
        payload["equity"] / (payload["debt"] + payload["equity"]) if (payload["debt"] + payload["equity"]) else 0
    )

    wacc = debt_ratio * payload["cost_of_debt"] * (1 - payload["tax_rate"]) + equity_ratio * payload["cost_of_equity"]

    invested_capital = payload["total_assets"] - payload["current_liabilities"]

    nopat = payload["ebit"] * (1 - payload["tax_rate"])

    capital_charge = invested_capital * wacc

    eva = nopat - capital_charge

    value_creation_index = (nopat / capital_charge) if capital_charge else 0

    wealth_created = eva > 0

    eva_momentum = "improving" if eva > 0 else "declining" if eva < -1000000 else "stable"

    return dict(
        company_id=payload["company_id"],
        period=payload["period"],
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


@app.post("/calculate", response_model=EVAResponse)
async def eva_calculation(request: EVARequest):
    logger.info("Calculating", company=request.company_id)
    result = calc.run(request.model_dump(), _reference)
    return EVAResponse(**result) if isinstance(result, dict) else result


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8212)
