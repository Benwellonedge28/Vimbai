"""
Asset Turnover Analysis Service
Port: 8216
Operating asset efficiency metrics
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
_spec = importlib.util.spec_from_file_location("asset_turnover_service_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["asset_turnover_service_calc"] = calc
_spec.loader.exec_module(calc)

app = FastAPI(title="Asset Turnover Analysis Service", version="1.0.0")


class TurnoverMetrics(BaseModel):
    total_asset_turnover: float
    fixed_asset_turnover: float
    working_capital_turnover: float
    receivables_turnover: float
    inventory_turnover: float
    payables_turnover: float
    cash_conversion_cycle: int


class TurnoverRequest(BaseModel):
    company_id: str
    period: str
    revenue: float
    cost_of_goods_sold: float
    total_assets: float
    fixed_assets: float
    current_assets: float
    current_liabilities: float
    accounts_receivable: float
    inventory: float
    accounts_payable: float


class TurnoverResponse(BaseModel):
    company_id: str
    period: str
    turnover_metrics: TurnoverMetrics
    industry_benchmark: Dict[str, float]
    efficiency_assessment: str
    improvement_areas: list
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
        "service": "asset-turnover",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    """The original Python calculation, kept as the oracle."""
    total_turnover = payload["revenue"] / payload["total_assets"] if payload["total_assets"] else 0
    fixed_turnover = payload["revenue"] / payload["fixed_assets"] if payload["fixed_assets"] else 0
    working_capital = payload["current_assets"] - payload["current_liabilities"]
    wc_turnover = payload["revenue"] / working_capital if working_capital else 0
    receivables_turnover = payload["revenue"] / payload["accounts_receivable"] if payload["accounts_receivable"] else 0
    inventory_turnover = payload["cost_of_goods_sold"] / payload["inventory"] if payload["inventory"] else 0
    payables_turnover = (
        payload["cost_of_goods_sold"] / payload["accounts_payable"] if payload["accounts_payable"] else 0
    )

    dso = (payload["accounts_receivable"] / payload["revenue"]) * 365 if payload["revenue"] else 0
    dio = (payload["inventory"] / payload["cost_of_goods_sold"]) * 365 if payload["cost_of_goods_sold"] else 0
    dpo = (payload["accounts_payable"] / payload["cost_of_goods_sold"]) * 365 if payload["cost_of_goods_sold"] else 0
    ccc = int(dso + dio - dpo)

    benchmark = {
        "total_asset_turnover": 1.5,
        "fixed_asset_turnover": 3.0,
        "receivables_turnover": 8.0,
        "inventory_turnover": 6.0,
    }

    improvement_areas = []
    if total_turnover < benchmark["total_asset_turnover"]:
        improvement_areas.append("Total asset utilization below industry average")
    if inventory_turnover < benchmark["inventory_turnover"]:
        improvement_areas.append("Inventory management could be improved")
    if ccc > 90:
        improvement_areas.append("Cash conversion cycle is lengthy")

    assessment = (
        "efficient"
        if total_turnover > benchmark["total_asset_turnover"]
        else "moderate" if total_turnover > 1.0 else "inefficient"
    )

    return dict(
        company_id=payload["company_id"],
        period=payload["period"],
        turnover_metrics=TurnoverMetrics(
            total_asset_turnover=round(total_turnover, 2),
            fixed_asset_turnover=round(fixed_turnover, 2),
            working_capital_turnover=round(wc_turnover, 2),
            receivables_turnover=round(receivables_turnover, 2),
            inventory_turnover=round(inventory_turnover, 2),
            payables_turnover=round(payables_turnover, 2),
            cash_conversion_cycle=ccc,
        ),
        industry_benchmark=benchmark,
        efficiency_assessment=assessment,
        improvement_areas=(
            improvement_areas if improvement_areas else ["Asset utilization is in line with expectations"]
        ),
        recommendations=(
            [
                "Optimize working capital management",
                "Improve inventory turnover",
                "Review credit policies for receivables",
                "Negotiate better payment terms with suppliers",
            ]
            if assessment == "inefficient"
            else ["Continue monitoring efficiency metrics"]
        ),
    )


@app.post("/analyze", response_model=TurnoverResponse)
async def asset_turnover_analysis(request: TurnoverRequest):
    logger.info("Calculating", company=request.company_id)
    result = calc.run(request.model_dump(), _reference)
    return TurnoverResponse(**result) if isinstance(result, dict) else result


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8216)
