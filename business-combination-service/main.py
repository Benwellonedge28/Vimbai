"""
Business Combination Service
Port: 8384
Purchase price allocation
"""
import httpx
import structlog
from typing import Any, Dict, List
from datetime import datetime
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Business Combination Service", version="1.0.0")

class AcquisitionRequest(BaseModel):
    company_id: str
    target_id: str
    purchase_price: float
    net_assets_acquired: Dict[str, float]
    fair_value_adjustments: Dict[str, float]

class AcquisitionResponse(BaseModel):
    target_id: str
    purchase_price: float
    goodwill: float
    net_assets_fair_value: float
    fair_value_adjustments: Dict[str, float]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "business-combination", "version": "1.0.0"}

@app.post("/allocate", response_model=AcquisitionResponse)
async def allocate_purchase_price(request: AcquisitionRequest):
    logger.info("Allocating purchase price", company=request.company_id, target=request.target_id)
    
    nav = sum(request.net_assets_acquired.values())
    adjustments = sum(request.fair_value_adjustments.values())
    goodwill = request.purchase_price - nav - adjustments
    
    return AcquisitionResponse(
        target_id=request.target_id,
        purchase_price=round(request.purchase_price, 2),
        goodwill=round(max(0, goodwill), 2),
        net_assets_fair_value=round(nav + adjustments, 2),
        fair_value_adjustments={k: round(v, 2) for k, v in request.fair_value_adjustments.items()}
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8384)
