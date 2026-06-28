"""
Divestiture Service
Port: 8385
Asset disposal accounting
"""
import httpx
import structlog
from typing import Any, Dict, List
from datetime import datetime
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Divestiture Service", version="1.0.0")

class DivestitureRequest(BaseModel):
    company_id: str
    asset_id: str
    disposal_proceeds: float
    carrying_value: float
    cumulative_ota: float

class DivestitureResponse(BaseModel):
    asset_id: str
    proceeds: float
    carrying_value: float
    gain_loss: float
    tax_impact: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "divestiture", "version": "1.0.0"}

@app.post("/calculate", response_model=DivestitureResponse)
async def calculate_disposal(request: DivestitureRequest):
    logger.info("Calculating divestiture", company=request.company_id, asset=request.asset_id)
    
    net_book = request.carrying_value - request.cumulative_ota
    gain_loss = request.disposal_proceeds - net_book
    tax = max(0, gain_loss * 0.21)
    
    return DivestitureResponse(
        asset_id=request.asset_id,
        proceeds=round(request.disposal_proceeds, 2),
        carrying_value=round(net_book, 2),
        gain_loss=round(gain_loss, 2),
        tax_impact=round(tax, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8385)
