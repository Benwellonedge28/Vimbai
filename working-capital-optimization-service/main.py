"""
Working Capital Optimization Service
Port: 8168
DSO, DPO, DIO optimization, cash conversion cycle improvement
"""

from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

logger = structlog.get_logger()
app = FastAPI(title="Working Capital Optimization Service", version="1.0.0")


class WorkingCapitalRequest(BaseModel):
    company_id: str
    reporting_date: str
    accounts_receivable: float
    accounts_payable: float
    inventory: float
    revenue: float
    cogs: float
    purchases: float
    days_analysis: int = 365


class WorkingCapitalResponse(BaseModel):
    company_id: str
    dso: float
    dpo: float
    dio: float
    cash_conversion_cycle: float
    working_capital: float
    current_ratio: float
    quick_ratio: float
    net_working_capital: float
    recommendations: Dict[str, str]


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
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
    return {"status": "healthy", "service": "working-capital-optimization", "version": "1.0.0"}


@app.post("/optimize", response_model=WorkingCapitalResponse)
async def optimize_working_capital(request: WorkingCapitalRequest):
    logger.info("Optimizing working capital", company=request.company_id)

    dso = (request.accounts_receivable / request.revenue) * request.days_analysis
    dpo = (request.accounts_payable / request.purchases) * request.days_analysis
    dio = (request.inventory / request.cogs) * request.days_analysis
    ccc = dso + dio - dpo

    recommendations = {}
    if dso > 60:
        recommendations["DSO"] = "Consider tighter credit policies, early payment discounts, or invoice factoring"
    else:
        recommendations["DSO"] = "DSO is within acceptable range"

    if dpo < 45:
        recommendations["DPO"] = "Negotiate extended payment terms with suppliers to improve cash flow"
    else:
        recommendations["DPO"] = "DPO is well managed"

    if dio > 90:
        recommendations["DIO"] = (
            "Implement just-in-time inventory, review slow-moving stock, improve demand forecasting"
        )
    else:
        recommendations["DIO"] = "Inventory turnover is satisfactory"

    current_assets = request.accounts_receivable + request.inventory
    working_capital = current_assets - request.accounts_payable

    return WorkingCapitalResponse(
        company_id=request.company_id,
        dso=round(dso, 1),
        dpo=round(dpo, 1),
        dio=round(dio, 1),
        cash_conversion_cycle=round(ccc, 1),
        working_capital=working_capital,
        current_ratio=round(current_assets / request.accounts_payable, 2),
        quick_ratio=round(request.accounts_receivable / request.accounts_payable, 2),
        net_working_capital=working_capital,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8168)
