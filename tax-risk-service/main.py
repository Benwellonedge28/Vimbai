"""
Tax Risk Service
Port: 8296
Tax risk assessment
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Tax Risk Service", version="1.0.0")

class TaxRiskRequest(BaseModel):
    company_id: str
    tax_positions: List[Dict[str, Any]]
    audit_history: List[Dict[str, Any]]

class TaxRiskResponse(BaseModel):
    company_id: str
    risk_summary: Dict[str, Any]
    risk_factors: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "tax-risk", "version": "1.0.0"}

@app.post("/assess", response_model=TaxRiskResponse)
async def assess_tax_risk(request: TaxRiskRequest):
    logger.info("Assessing tax risk", company=request.company_id)

    uncertain_positions = sum(1 for p in request.tax_positions if p.get("uncertain", False))
    audit_adjustments = sum(abs(a.get("adjustment", 0)) for a in request.audit_history)
    
    risk_summary = {
        "total_positions": len(request.tax_positions),
        "uncertain_positions": uncertain_positions,
        "total_tax_at_risk": uncertain_positions * 50000,
        "prior_audit_adjustments": audit_adjustments,
        "risk_level": "High" if uncertain_positions > 3 else "Medium" if uncertain_positions > 1 else "Low"
    }
    
    risk_factors = []
    if uncertain_positions > 0:
        risk_factors.append(f"{uncertain_positions} uncertain tax positions require review")
    if audit_adjustments > 100000:
        risk_factors.append("History of significant audit adjustments")
    
    return TaxRiskResponse(company_id=request.company_id, risk_summary=risk_summary, risk_factors=risk_factors)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8296)
