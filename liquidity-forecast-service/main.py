"""
Liquidity Forecast Service
Port: 8260
Short-term liquidity forecasting
"""

from datetime import datetime, timedelta
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Liquidity Forecast Service", version="1.0.0")


class CashFlow(BaseModel):
    date: str
    inflow: float
    outflow: float
    net: float


class LiquidityForecastRequest(BaseModel):
    company_id: str
    starting_cash: float
    cash_flows: List[CashFlow]
    credit_lines_available: float
    minimum_cash_requirement: float


class LiquidityForecastResponse(BaseModel):
    company_id: str
    forecast_summary: Dict[str, Any]
    daily_forecast: List[Dict[str, Any]]
    liquidity_gaps: List[Dict[str, Any]]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "liquidity-forecast", "version": "1.0.0"}


@app.post("/forecast", response_model=LiquidityForecastResponse)
async def forecast_liquidity(request: LiquidityForecastRequest):
    logger.info("Forecasting liquidity", company=request.company_id)

    running_balance = request.starting_cash
    daily_forecast = []
    min_balance = request.starting_cash

    for cf in request.cash_flows:
        running_balance += cf.net
        min_balance = min(min_balance, running_balance)
        daily_forecast.append(
            {
                "date": cf.date,
                "inflow": cf.inflow,
                "outflow": cf.outflow,
                "net": cf.net,
                "balance": round(running_balance, 2),
            }
        )

    total_inflows = sum(cf["inflow"] for cf in daily_forecast)
    total_outflows = sum(cf["outflow"] for cf in daily_forecast)
    avg_daily_outflow = total_outflows / len(daily_forecast) if daily_forecast else 0

    liquidity_gaps = []
    for cf in daily_forecast:
        if cf["balance"] < request.minimum_cash_requirement:
            shortfall = request.minimum_cash_requirement - cf["balance"]
            liquidity_gaps.append(
                {"date": cf["date"], "shortfall": round(shortfall, 2), "required_credit": round(shortfall, 2)}
            )

    forecast_summary = {
        "starting_cash": request.starting_cash,
        "ending_cash": round(running_balance, 2),
        "total_inflows": round(total_inflows, 2),
        "total_outflows": round(total_outflows, 2),
        "net_change": round(running_balance - request.starting_cash, 2),
        "min_balance": round(min_balance, 2),
        "credit_available": request.credit_lines_available,
    }

    recommendations = []
    if min_balance < request.minimum_cash_requirement:
        recommendations.append("Liquidity gap identified - draw on credit facilities")
    if forecast_summary["ending_cash"] < request.starting_cash * 0.5:
        recommendations.append("Significant cash depletion - monitor closely")

    return LiquidityForecastResponse(
        company_id=request.company_id,
        forecast_summary=forecast_summary,
        daily_forecast=daily_forecast,
        liquidity_gaps=liquidity_gaps,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8260)
