"""
Insurance Claims Service
Port: 8367
Insurance claim tracking and accounting
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Insurance Claims Service", version="1.0.0")

class ClaimRequest(BaseModel):
    company_id: str
    policy_number: str
    claim_amount: float
    claim_type: str
    incident_date: date

class ClaimResponse(BaseModel):
    claim_id: str
    status: str
    approved_amount: float
    deductible: float
    payout_amount: float

class ReserveRequest(BaseModel):
    company_id: str
    claim_id: str
    reserve_amount: float
    reserve_type: str

class ReserveResponse(BaseModel):
    claim_id: str
    total_reserve: float
    paid_to_date: float
    remaining_reserve: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "insurance-claims", "version": "1.0.0"}

@app.post("/submit", response_model=ClaimResponse)
async def submit_claim(request: ClaimRequest):
    logger.info("Submitting claim", company=request.company_id, policy=request.policy_number)
    
    deductible = 1000.0
    approved = request.claim_amount - deductible
    
    return ClaimResponse(
        claim_id=f"CLM-{datetime.now().strftime('%Y%m%d%H%M')}",
        status="approved",
        approved_amount=round(approved, 2),
        deductible=deductible,
        payout_amount=round(approved, 2)
    )

@app.post("/reserve", response_model=ReserveResponse)
async def set_reserve(request: ReserveRequest):
    logger.info("Setting reserve", claim=request.claim_id)
    
    return ReserveResponse(
        claim_id=request.claim_id,
        total_reserve=request.reserve_amount,
        paid_to_date=0.0,
        remaining_reserve=request.reserve_amount
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8367)
