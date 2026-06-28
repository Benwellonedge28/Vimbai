"""
Goodwill Service
Port: 8359
Goodwill tracking and impairment testing
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Goodwill Service", version="1.0.0")

class GoodwillRequest(BaseModel):
    company_id: str
    reporting_units: List[Dict[str, Any]]
    testing_date: date

class GoodwillResponse(BaseModel):
    company_id: str
    total_goodwill: float
    total_impairment: float
    net_goodwill: float
    unit_details: List[Dict[str, Any]]

class GoodwillAllocationRequest(BaseModel):
    acquisition_id: str
    total_purchase_price: float
    fair_value_assets: Dict[str, float]
    fair_value_liabilities: Dict[str, float]

class GoodwillAllocationResponse(BaseModel):
    acquisition_id: str
    total_goodwill: float
    allocated_to_assets: Dict[str, float]
    step_up_adjustments: Dict[str, float]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "goodwill", "version": "1.0.0"}

@app.post("/assess", response_model=GoodwillResponse)
async def assess_goodwill(request: GoodwillRequest):
    logger.info("Assessing goodwill", company=request.company_id)
    
    total_gw = sum(u.get("goodwill_amount", 0) for u in request.reporting_units)
    total_imp = sum(u.get("impairment_charges", 0) for u in request.reporting_units)
    
    return GoodwillResponse(
        company_id=request.company_id,
        total_goodwill=round(total_gw, 2),
        total_impairment=round(total_imp, 2),
        net_goodwill=round(total_gw - total_imp, 2),
        unit_details=request.reporting_units
    )

@app.post("/allocate", response_model=GoodwillAllocationResponse)
async def allocate_goodwill(request: GoodwillAllocationRequest):
    logger.info("Allocating goodwill", acquisition=request.acquisition_id)
    
    asset_fv = sum(request.fair_value_assets.values())
    liability_fv = sum(request.fair_value_liabilities.values())
    goodwill = request.total_purchase_price - (asset_fv - liability_fv)
    
    return GoodwillAllocationResponse(
        acquisition_id=request.acquisition_id,
        total_goodwill=round(max(0, goodwill), 2),
        allocated_to_assets={},
        step_up_adjustments={}
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8359)
