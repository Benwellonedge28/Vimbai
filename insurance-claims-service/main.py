"""
Vimbai Insurance Claims Service
Claims processing, coverage validation, settlement calculation, and claims tracking.
Port: 8371
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
from enum import Enum
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

SERVICE_NAME = "insurance-claims-service"
PORT = int(os.getenv("PORT", "8371"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Insurance Claims Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class ClaimStatus(str, Enum):
    FILED = "filed"; UNDER_REVIEW = "under_review"; APPROVED = "approved"; DENIED = "denied"; PAID = "paid"

class InsuranceClaim(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; policy_number: str; claim_type: str  # property, liability, auto, health, business_interruption
    incident_date: str; claim_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    claim_amount: float; deductible: float = 0
    description: str = ""; supporting_docs: List[str] = []
    coverage_limit: float = 0
    status: ClaimStatus = ClaimStatus.FILED

class ClaimResult(BaseModel):
    claim_id: str; company_id: str; status: ClaimStatus
    covered_amount: float; deductible_applied: float; settlement_amount: float
    coverage_ratio: float; notes: str = ""

_claims: Dict[str, List[InsuranceClaim]] = {}

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/file", response_model=InsuranceClaim)
async def file_claim(claim: InsuranceClaim):
    _claims.setdefault(claim.company_id, []).append(claim)
    logger.info("Claim filed", claim_id=claim.id, company=claim.company_id)
    return claim

@app.get("/claims", response_model=List[InsuranceClaim])
async def list_claims(company_id: str, status: str = ""):
    claims = _claims.get(company_id, [])
    if status:
        claims = [c for c in claims if c.status.value == status]
    return claims

@app.post("/claims/{claim_id}/process", response_model=ClaimResult)
async def process_claim(company_id: str, claim_id: str):
    claims = _claims.get(company_id, [])
    claim = next((c for c in claims if c.id == claim_id), None)
    if not claim:
        raise HTTPException(status_code=404, detail="Claim not found")
    
    claim.status = ClaimStatus.UNDER_REVIEW
    
    covered = claim.claim_amount
    if claim.coverage_limit > 0:
        covered = min(covered, claim.coverage_limit)
    covered -= claim.deductible
    covered = max(covered, 0)
    
    coverage_ratio = covered / claim.claim_amount if claim.claim_amount else 0
    
    claim.status = ClaimStatus.APPROVED if covered > 0 else ClaimStatus.DENIED
    
    return ClaimResult(
        claim_id=claim.id, company_id=company_id, status=claim.status,
        covered_amount=round(covered, 2), deductible_applied=claim.deductible,
        settlement_amount=round(covered, 2), coverage_ratio=round(coverage_ratio, 4),
        notes=f"Coverage limit: {claim.coverage_limit}, Deductible: {claim.deductible}"
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
