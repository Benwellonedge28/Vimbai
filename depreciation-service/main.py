"""
Depreciation Service
Port: 8356
Depreciation calculation and scheduling
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Depreciation Service", version="1.0.0")

class DepreciationRequest(BaseModel):
    company_id: str
    assets: List[Dict[str, Any]]
    method: str
    period_start: date
    period_end: date

class DepreciationResponse(BaseModel):
    company_id: str
    period: Dict[str, date]
    total_depreciation: float
    asset_depreciation: List[Dict[str, Any]]
    schedule: List[Dict[str, Any]]

class DepreciationMethodRequest(BaseModel):
    cost: float
    salvage_value: float
    useful_life: int
    method: str

class DepreciationMethodResponse(BaseModel):
    method: str
    annual_depreciation: float
    monthly_depreciation: float
    depreciation_schedule: List[Dict[str, Any]]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "depreciation", "version": "1.0.0"}

@app.post("/calculate", response_model=DepreciationResponse)
async def calculate_depreciation(request: DepreciationRequest):
    logger.info("Calculating depreciation", company=request.company_id, method=request.method)
    
    total_depr = 0.0
    asset_depr = []
    schedule = []
    
    for asset in request.assets:
        cost = asset.get("cost", 0)
        life = asset.get("useful_life_years", 5)
        depr_method = request.method
        
        if depr_method == "straight_line":
            annual = (cost - asset.get("salvage", 0)) / life
        elif depr_method == "declining_balance":
            rate = 2 / life
            annual = cost * rate
        else:
            annual = cost / life
        
        total_depr += annual
        asset_depr.append({
            "asset_id": asset.get("asset_id"),
            "annual_depreciation": round(annual, 2),
            "accumulated": round(annual, 2)
        })
    
    return DepreciationResponse(
        company_id=request.company_id,
        period={"start": request.period_start, "end": request.period_end},
        total_depreciation=round(total_depr, 2),
        asset_depreciation=asset_depr,
        schedule=[{"year": y, "depreciation": round(total_depr * 0.9, 2)} for y in range(1, 6)]
    )

@app.post("/method", response_model=DepreciationMethodResponse)
async def calculate_depreciation_method(request: DepreciationMethodRequest):
    logger.info("Calculating depreciation method", method=request.method)
    
    depreciable = request.cost - request.salvage_value
    
    if request.method == "straight_line":
        annual = depreciable / request.useful_life
    elif request.method == "double_declining":
        rate = 2 / request.useful_life
        annual = request.cost * rate
    else:
        annual = depreciable / request.useful_life
    
    schedule = []
    remaining = depreciable
    for y in range(request.useful_life):
        remaining -= annual
        schedule.append({"year": y + 1, "depreciation": round(annual, 2), "accumulated": round(annual * (y + 1), 2), "book_value": round(max(0, request.salvage_value + remaining), 2)})
    
    return DepreciationMethodResponse(
        method=request.method,
        annual_depreciation=round(annual, 2),
        monthly_depreciation=round(annual / 12, 2),
        depreciation_schedule=schedule
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8356)
