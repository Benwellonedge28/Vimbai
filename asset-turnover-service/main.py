"""
Asset Turnover Analysis Service
Port: 8216
Operating asset efficiency metrics
"""

from typing import Any, Dict

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
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
    return {"status": "healthy", "service": "asset-turnover", "version": "1.0.0"}


@app.post("/analyze", response_model=TurnoverResponse)
async def analyze_asset_turnover(request: TurnoverRequest):
    logger.info("Analyzing asset turnover", company=request.company_id, period=request.period)

    total_turnover = request.revenue / request.total_assets if request.total_assets else 0
    fixed_turnover = request.revenue / request.fixed_assets if request.fixed_assets else 0
    working_capital = request.current_assets - request.current_liabilities
    wc_turnover = request.revenue / working_capital if working_capital else 0
    receivables_turnover = request.revenue / request.accounts_receivable if request.accounts_receivable else 0
    inventory_turnover = request.cost_of_goods_sold / request.inventory if request.inventory else 0
    payables_turnover = request.cost_of_goods_sold / request.accounts_payable if request.accounts_payable else 0

    dso = (request.accounts_receivable / request.revenue) * 365 if request.revenue else 0
    dio = (request.inventory / request.cost_of_goods_sold) * 365 if request.cost_of_goods_sold else 0
    dpo = (request.accounts_payable / request.cost_of_goods_sold) * 365 if request.cost_of_goods_sold else 0
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

    return TurnoverResponse(
        company_id=request.company_id,
        period=request.period,
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


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8216)
