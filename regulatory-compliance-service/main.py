"""
Regulatory Compliance Service
Port: 8285
Regulatory compliance assessment
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Regulatory Compliance Service", version="1.0.0")

class ComplianceRequirement(BaseModel):
    requirement_id: str
    regulation: str
    description: str
    status: str

class RegulatoryComplianceRequest(BaseModel):
    company_id: str
    requirements: List[ComplianceRequirement]

class RegulatoryComplianceResponse(BaseModel):
    company_id: str
    assessment_date: str
    compliance_summary: Dict[str, Any]
    requirements_status: List[Dict[str, Any]]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "regulatory-compliance", "version": "1.0.0"}

@app.post("/assess", response_model=RegulatoryComplianceResponse)
async def assess_regulatory_compliance(request: RegulatoryComplianceRequest):
    logger.info("Assessing regulatory compliance", company=request.company_id)

    compliant = sum(1 for r in request.requirements if r.status == "Compliant")
    
    requirements_status = [
        {"id": r.requirement_id, "regulation": r.regulation, "status": r.status}
        for r in request.requirements
    ]
    
    compliance_summary = {
        "total_requirements": len(request.requirements),
        "compliant": compliant,
        "non_compliant": len(request.requirements) - compliant,
        "compliance_rate": round(compliant / len(request.requirements) * 100, 2) if request.requirements else 0
    }
    
    return RegulatoryComplianceResponse(
        company_id=request.company_id,
        assessment_date=datetime.now().isoformat(),
        compliance_summary=compliance_summary,
        requirements_status=requirements_status
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8285)
