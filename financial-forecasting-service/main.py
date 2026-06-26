"""
Financial Forecasting Service
Port: 8287
Financial projections and forecasting
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Financial Forecasting Service", version="1.0.0")

class ForecastAssumption(BaseModel):
    assumption_name: str
    base_value: float
    growth_rate: float
    period: int

class FinancialForecastingRequest(BaseModel):
    company_id: str
    assumptions: List[ForecastAssumption]
    forecast_years: int

class FinancialForecastingResponse(BaseModel):
    company_id: str
    forecast_summary: Dict[str, Any]
    projections: List[Dict[str, Any]]
    scenarios: Dict[str, Any]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "financial-forecasting", "version": "1.0.0"}

@app.post("/forecast", response_model=FinancialForecastingResponse)
async def forecast_financials(request: FinancialForecastingRequest):
    logger.info("Forecasting financials", company=request.company_id)

    projections = []
    base_values = {a.assumption_name: a.base_value for a in request.assumptions}
    
    for year in range(1, request.forecast_years + 1):
        projection = {"year": year}
        for a in request.assumptions:
            projection[a.assumption_name] = round(a.base_value * ((1 + a.growth_rate) ** year), 2)
        projections.append(projection)
    
    forecast_summary = {
        "base_year": projections[0] if projections else {},
        "final_year": projections[-1] if projections else {},
        "cagr": round((projections[-1].get(list(base_values.keys())[0], 0) / projections[0].get(list(base_values.keys())[0], 1) - 1) * 100, 2) if projections and projections[0] else 0
    }
    
    scenarios = {
        "base": projections[-1] if projections else {},
        "optimistic": {k: v * 1.2 for k, v in (projections[-1].items() if projections else {}.items())},
        "pessimistic": {k: v * 0.8 for k, v in (projections[-1].items() if projections else {}.items())}
    }
    
    return FinancialForecastingResponse(
        company_id=request.company_id,
        forecast_summary=forecast_summary,
        projections=projections,
        scenarios=scenarios
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8287)
