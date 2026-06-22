"""
Cash Flow Forecasting Service
Port: 8167
Short-term and long-term cash flow projections, scenario analysis
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Cash Flow Forecasting Service", version="1.0.0")

class CashFlowItem(BaseModel):
    item_id: str
    category: str
    description: str
    amount: float
    frequency: str
    certainty: str

class ForecastRequest(BaseModel):
    company_id: str
    forecast_start_date: str
    forecast_periods: int = Field(default=12, ge=1, le=36)
    opening_cash: float
    inflow_items: List[CashFlowItem]
    outflow_items: List[CashFlowItem]
    scenario: str = "base"

class ForecastPeriod(BaseModel):
    period: str
    inflows: float
    outflows: float
    net_flow: float
    closing_cash: float

class ForecastResponse(BaseModel):
    company_id: str
    forecast_start_date: str
    periods: List[ForecastPeriod]
    total_inflows: float
    total_outflows: float
    net_cash_flow: float
    minimum_cash_balance: float
    maximum_cash_balance: float
    financing_required: float
    peak_funding_requirement: float

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
    return {"status": "healthy", "service": "cash-flow-forecasting", "version": "1.0.0"}

@app.post("/forecast", response_model=ForecastResponse)
async def forecast_cash_flows(request: ForecastRequest):
    logger.info("Forecasting cash flows", company=request.company_id, periods=request.forecast_periods)

    certainty_multipliers = {"high": 1.0, "medium": 0.8, "low": 0.5}

    if request.scenario == "stress":
        certainty_multipliers = {k: v * 0.7 for k, v in certainty_multipliers.items()}

    periods = []
    closing = request.opening_cash
    min_cash = closing
    max_cash = closing
    financing_needed = 0.0

    for i in range(1, request.forecast_periods + 1):
        inflows = sum(item.amount * certainty_multipliers.get(item.certainty, 0.8) for item in request.inflow_items)
        outflows = sum(item.amount * certainty_multipliers.get(item.certainty, 0.9) for item in request.outflow_items)
        net_flow = inflows - outflows
        closing = closing + net_flow

        if closing < 0:
            financing_needed += abs(closing)
            closing = 0

        min_cash = min(min_cash, closing)
        max_cash = max(max_cash, closing)

        periods.append(ForecastPeriod(
            period=f"Month {i}",
            inflows=inflows,
            outflows=outflows,
            net_flow=net_flow,
            closing_cash=closing
        ))

    return ForecastResponse(
        company_id=request.company_id,
        forecast_start_date=request.forecast_start_date,
        periods=periods,
        total_inflows=sum(p.inflows for p in periods),
        total_outflows=sum(p.outflows for p in periods),
        net_cash_flow=sum(p.net_flow for p in periods),
        minimum_cash_balance=min_cash,
        maximum_cash_balance=max_cash,
        financing_required=financing_needed,
        peak_funding_requirement=financing_needed
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8167)
