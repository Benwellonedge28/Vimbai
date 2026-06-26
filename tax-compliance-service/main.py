"""
Tax Compliance Service
Port: 8293
Tax filing and compliance
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Tax Compliance Service", version="1.0.0")

class TaxFiling(BaseModel):
    filing_id: str
    tax_type: str
    jurisdiction: str
    due_date: str
    status: str

class TaxComplianceRequest(BaseModel):
    company_id: str
    filings: List[TaxFiling]

class TaxComplianceResponse(BaseModel):
    company_id: str
    compliance_summary: Dict[str, Any]
    upcoming_deadlines: List[Dict[str, Any]]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "tax-compliance", "version": "1.0.0"}

@app.post("/check", response_model=TaxComplianceResponse)
async def check_compliance(request: TaxComplianceRequest):
    logger.info("Checking tax compliance", company=request.company_id)

    on_time = sum(1 for f in request.filings if f.status == "Filed On Time")
    
    compliance_summary = {
        "total_filings": len(request.filings),
        "on_time": on_time,
        "pending": sum(1 for f in request.filings if f.status == "Pending"),
        "compliance_rate": round(on_time / len(request.filings) * 100, 2) if request.filings else 0
    }
    
    upcoming_deadlines = [
        {"type": f.tax_type, "jurisdiction": f.jurisdiction, "due": f.due_date}
        for f in request.filings if f.status == "Pending"
    ]
    
    return TaxComplianceResponse(
        company_id=request.company_id,
        compliance_summary=compliance_summary,
        upcoming_deadlines=upcoming_deadlines
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8293)
