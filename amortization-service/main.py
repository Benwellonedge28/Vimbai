"""
Amortization Service
Port: 8357
Intangible assets amortization
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Amortization Service", version="1.0.0")

class AmortizationRequest(BaseModel):
    company_id: str
    intangibles: List[Dict[str, Any]]
    period_start: date
    period_end: date

class AmortizationResponse(BaseModel):
    company_id: str
    period: Dict[str, date]
    total_amortization: float
    intangible_amortization: List[Dict[str, Any]]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "amortization", "version": "1.0.0"}

@app.post("/calculate", response_model=AmortizationResponse)
async def calculate_amortization(request: AmortizationRequest):
    logger.info("Calculating amortization", company=request.company_id)
    
    total_amort = 0.0
    intangible_amort = []
    
    for intangible in request.intangibles:
        cost = intangible.get("cost", 0)
        life = intangible.get("useful_life_years", 10)
        annual = cost / life
        total_amort += annual
        
        intangible_amort.append({
            "intangible_id": intangible.get("intangible_id"),
            "type": intangible.get("type", "Other"),
            "cost": cost,
            "annual_amortization": round(annual, 2)
        })
    
    return AmortizationResponse(
        company_id=request.company_id,
        period={"start": request.period_start, "end": request.period_end},
        total_amortization=round(total_amort, 2),
        intangible_amortization=intangible_amort
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8357)
