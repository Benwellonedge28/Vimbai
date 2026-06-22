"""
Insurance Claims Service
Port: 8221
Insurance claim processing and accounting
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Insurance Claims Service", version="1.0.0")

class ClaimItem(BaseModel):
    claim_id: str
    policy_number: str
    claim_type: str
    claim_amount: float
    deductible: float
    reserves: float
    paid_amount: float
    status: str

class InsuranceClaimsRequest(BaseModel):
    company_id: str
    period: str
    claims: List[Dict[str, Any]]
    premiums_paid: float

class InsuranceClaimsResponse(BaseModel):
    company_id: str
    period: str
    claim_items: List[ClaimItem]
    total_claims: float
    total_reserves: float
    total_paid: float
    claims_ratio: float
    loss_ratio: float
    recommendations: List[str]

async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
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
    return {"status": "healthy", "service": "insurance-claims", "version": "1.0.0"}

@app.post("/analyze", response_model=InsuranceClaimsResponse)
async def analyze_insurance_claims(request: InsuranceClaimsRequest):
    logger.info("Analyzing insurance claims", company=request.company_id, period=request.period)

    claim_items = []
    total_claims = 0.0
    total_reserves = 0.0
    total_paid = 0.0

    for claim in request.claims:
        amount = claim.get("claim_amount", 0)
        deductible = claim.get("deductible", 0)
        reserves = claim.get("reserves", 0)
        paid = claim.get("paid_amount", 0)

        total_claims += amount
        total_reserves += reserves
        total_paid += paid

        claim_items.append(ClaimItem(
            claim_id=claim.get("id", ""),
            policy_number=claim.get("policy", ""),
            claim_type=claim.get("type", ""),
            claim_amount=amount,
            deductible=deductible,
            reserves=reserves,
            paid_amount=paid,
            status=claim.get("status", "open")
        ))

    claims_ratio = (total_claims / request.premiums_paid) if request.premiums_paid else 0
    loss_ratio = (total_paid / request.premiums_paid) if request.premiums_paid else 0

    return InsuranceClaimsResponse(
        company_id=request.company_id,
        period=request.period,
        claim_items=claim_items,
        total_claims=round(total_claims, 2),
        total_reserves=round(total_reserves, 2),
        total_paid=round(total_paid, 2),
        claims_ratio=round(claims_ratio, 4),
        loss_ratio=round(loss_ratio, 4),
        recommendations=["Review claims processing efficiency", "Monitor loss ratio trends"] if loss_ratio > 0.6 else ["Loss ratio within acceptable range"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8221)
