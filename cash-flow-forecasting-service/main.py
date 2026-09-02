"""
Cash Flow Forecasting Service
Port: 8238
Cash flow projection and forecasting
"""

import json
from datetime import datetime, timedelta
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Cash Flow Forecasting Service", version="1.0.0")


class CashFlowItem(BaseModel):
    category: str
    amount: float
    frequency: str
    certainty: str


class CashFlowForecastRequest(BaseModel):
    company_id: str
    starting_cash: float
    inflows: List[CashFlowItem]
    outflows: List[CashFlowItem]
    forecast_period_days: int
    scenario: str


class CashFlowForecastResponse(BaseModel):
    company_id: str
    forecast_date: str
    period_days: int
    daily_forecasts: List[Dict[str, Any]]
    weekly_summary: List[Dict[str, Any]]
    monthly_summary: List[Dict[str, Any]]
    ending_cash: float
    peak_cash_needed: float
    min_cash_balance: float
    cash_burn_rate: float
    runway_days: Optional[float]
    scenarios: Dict[str, Any]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "cash-flow-forecasting", "version": "1.0.0"}


@app.post("/forecast", response_model=CashFlowForecastResponse)
async def forecast_cash_flow(request: CashFlowForecastRequest):
    logger.info("Forecasting cash flow", company=request.company_id, days=request.forecast_period_days)

    daily_forecasts = []
    cash_balance = request.starting_cash
    min_cash = request.starting_cash
    peak_needed = 0

    for day in range(request.forecast_period_days):
        day_inflow = sum(
            item.amount
            for item in request.inflows
            if item.frequency in ["daily", "weekly"] and day % (7 if item.frequency == "weekly" else 1) == 0
        )
        day_outflow = sum(
            item.amount
            for item in request.outflows
            if item.frequency in ["daily", "weekly"] and day % (7 if item.frequency == "weekly" else 1) == 0
        )

        cash_balance += day_inflow - day_outflow
        min_cash = min(min_cash, cash_balance)

        daily_forecasts.append(
            {
                "day": day + 1,
                "inflow": round(day_inflow, 2),
                "outflow": round(day_outflow, 2),
                "net_flow": round(day_inflow - day_outflow, 2),
                "ending_cash": round(cash_balance, 2),
            }
        )

        if cash_balance < 0:
            peak_needed = max(peak_needed, abs(cash_balance))

    weekly_summary = []
    for w in range(0, request.forecast_period_days, 7):
        week_data = daily_forecasts[w : min(w + 7, len(daily_forecasts))]
        weekly_summary.append(
            {
                "week": w // 7 + 1,
                "total_inflow": round(sum(d["inflow"] for d in week_data), 2),
                "total_outflow": round(sum(d["outflow"] for d in week_data), 2),
                "ending_cash": week_data[-1]["ending_cash"] if week_data else request.starting_cash,
            }
        )

    monthly_summary = []
    for m in range(0, request.forecast_period_days, 30):
        month_data = daily_forecasts[m : min(m + 30, len(daily_forecasts))]
        monthly_summary.append(
            {
                "month": m // 30 + 1,
                "total_inflow": round(sum(d["inflow"] for d in month_data), 2),
                "total_outflow": round(sum(d["outflow"] for d in month_data), 2),
                "ending_cash": month_data[-1]["ending_cash"] if month_data else request.starting_cash,
            }
        )

    total_outflow = sum(d["outflow"] for d in daily_forecasts)
    burn_rate = total_outflow / request.forecast_period_days if request.forecast_period_days else 0
    runway = request.starting_cash / burn_rate if burn_rate > 0 else None

    scenarios = {
        "base": {"ending_cash": cash_balance, "peak_need": peak_needed},
        "optimistic": {"ending_cash": cash_balance * 1.2, "peak_need": peak_needed * 0.8},
        "pessimistic": {"ending_cash": cash_balance * 0.7, "peak_need": peak_needed * 1.3},
    }

    return CashFlowForecastResponse(
        company_id=request.company_id,
        forecast_date=datetime.now().isoformat(),
        period_days=request.forecast_period_days,
        daily_forecasts=daily_forecasts,
        weekly_summary=weekly_summary,
        monthly_summary=monthly_summary,
        ending_cash=round(cash_balance, 2),
        peak_cash_needed=round(peak_needed, 2),
        min_cash_balance=round(min_cash, 2),
        cash_burn_rate=round(burn_rate, 2),
        runway_days=round(runway, 2) if runway else None,
        scenarios=scenarios,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8238)
