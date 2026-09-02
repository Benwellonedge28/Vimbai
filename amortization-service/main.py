"""
Vimbai Amortization Service
Intangible asset amortization with straight-line and reducing balance methods.
Port: 8357
"""
import os, uuid, math
from datetime import datetime, timezone, date
from typing import Dict, List, Any
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "amortization-service"
PORT = int(os.getenv("PORT", "8357"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Amortization Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class IntangibleAsset(BaseModel):
    intangible_id: str; type: str  # patent, trademark, copyright, goodwill, software, license
    cost: float; accumulated_amortization: float = 0
    useful_life_years: int; residual_value: float = 0
    method: str = "straight_line"  # straight_line, reducing_balance
    rate: float = 0.25  # for reducing balance

class AmortizationRequest(BaseModel):
    company_id: str; intangibles: List[IntangibleAsset]
    period_start: date; period_end: date

class AmortizationSchedule(BaseModel):
    intangible_id: str; type: str; cost: float; residual_value: float
    annual_amortization: float; monthly_amortization: float
    accumulated_amortization: float; net_book_value: float
    method: str; remaining_life_years: float

class AmortizationResponse(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: Dict[str, str]
    total_amortization: float; total_net_book_value: float
    schedules: List[AmortizationSchedule] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/calculate", response_model=AmortizationResponse)
async def calculate_amortization(req: AmortizationRequest):
    total_amort = 0; total_nbv = 0
    schedules = []
    
    for asset in req.intangibles:
        depreciable_amount = asset.cost - asset.residual_value
        
        if asset.method == "straight_line":
            annual = depreciable_amount / asset.useful_life_years if asset.useful_life_years > 0 else 0
        else:  # reducing_balance
            annual = (asset.cost - asset.accumulated_amortization) * asset.rate
        
        monthly = annual / 12
        new_accumulated = asset.accumulated_amortization + annual
        nbv = asset.cost - new_accumulated
        remaining_years = nbv / annual if annual > 0 else 0
        
        total_amort += annual
        total_nbv += nbv
        
        schedules.append(AmortizationSchedule(
            intangible_id=asset.intangible_id, type=asset.type,
            cost=asset.cost, residual_value=asset.residual_value,
            annual_amortization=round(annual, 2), monthly_amortization=round(monthly, 2),
            accumulated_amortization=round(new_accumulated, 2), net_book_value=round(nbv, 2),
            method=asset.method, remaining_life_years=round(remaining_years, 1)
        ))
    
    return AmortizationResponse(
        company_id=req.company_id,
        period={"start": req.period_start.isoformat(), "end": req.period_end.isoformat()},
        total_amortization=round(total_amort, 2),
        total_net_book_value=round(total_nbv, 2),
        schedules=schedules
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
