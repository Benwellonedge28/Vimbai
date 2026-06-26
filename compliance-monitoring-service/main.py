"""
Compliance Monitoring Service
Port: 8282
Compliance monitoring and alerts
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Compliance Monitoring Service", version="1.0.0")

class ComplianceCheck(BaseModel):
    check_id: str
    regulation: str
    description: str
    status: str
    last_checked: str

class ComplianceMonitoringRequest(BaseModel):
    company_id: str
    checks: List[ComplianceCheck]

class ComplianceMonitoringResponse(BaseModel):
    company_id: str
    monitoring_date: str
    compliance_summary: Dict[str, Any]
    check_results: List[Dict[str, Any]]
    critical_issues: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "compliance-monitoring", "version": "1.0.0"}

@app.post("/monitor", response_model=ComplianceMonitoringResponse)
async def monitor_compliance(request: ComplianceMonitoringRequest):
    logger.info("Monitoring compliance", company=request.company_id)

    compliant = sum(1 for c in request.checks if c.status == "Compliant")
    non_compliant = sum(1 for c in request.checks if c.status == "Non-Compliant")
    pending = sum(1 for c in request.checks if c.status == "Pending")
    
    check_results = [
        {"check_id": c.check_id, "regulation": c.regulation, "status": c.status, "last_checked": c.last_checked}
        for c in request.checks
    ]
    
    critical_issues = [c.description for c in request.checks if c.status == "Non-Compliant"]
    
    compliance_summary = {
        "total_checks": len(request.checks),
        "compliant": compliant,
        "non_compliant": non_compliant,
        "pending": pending,
        "compliance_rate": round(compliant / len(request.checks) * 100, 2) if request.checks else 0
    }
    
    return ComplianceMonitoringResponse(
        company_id=request.company_id,
        monitoring_date=datetime.now().isoformat(),
        compliance_summary=compliance_summary,
        check_results=check_results,
        critical_issues=critical_issues
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8282)
