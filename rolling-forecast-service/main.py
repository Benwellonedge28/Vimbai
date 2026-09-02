"""
Rolling Forecast Service
Port: 8173
Rolling 12-month forecasts, continuous budgeting
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Rolling Forecast Service", version="1.0.0")


class ForecastMonth(BaseModel):
    month: str
    revenue: float
    expenses: float
    profit: float
    cash_flow: float


class RollingForecastRequest(BaseModel):
    company_id: str
    forecast_start: str
    months: int = 12
    base_revenue: float
    growth_rate: float
    base_expenses: float


class RollingForecastResponse(BaseModel):
    company_id: str
    forecast_start: str
    months: List[ForecastMonth]
    total_revenue: float
    total_expenses: float
    total_profit: float
    average_monthly_revenue: float


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
    return {"status": "healthy", "service": "rolling-forecast", "version": "1.0.0"}


@app.post("/forecast", response_model=RollingForecastResponse)
async def create_rolling_forecast(request: RollingForecastRequest):
    logger.info("Creating rolling forecast", company=request.company_id)

    months_data = []
    for i in range(request.months):
        growth = (1 + request.growth_rate) ** i
        revenue = request.base_revenue * growth
        expenses = request.base_expenses * growth * 0.75
        profit = revenue - expenses
        cash_flow = profit * 0.7

        months_data.append(
            ForecastMonth(
                month=f"Month {i+1}",
                revenue=round(revenue, 2),
                expenses=round(expenses, 2),
                profit=round(profit, 2),
                cash_flow=round(cash_flow, 2),
            )
        )

    return RollingForecastResponse(
        company_id=request.company_id,
        forecast_start=request.forecast_start,
        months=months_data,
        total_revenue=sum(m.revenue for m in months_data),
        total_expenses=sum(m.expenses for m in months_data),
        total_profit=sum(m.profit for m in months_data),
        average_monthly_revenue=sum(m.revenue for m in months_data) / len(months_data),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8173)
