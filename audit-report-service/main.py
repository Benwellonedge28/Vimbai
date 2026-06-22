"""
Audit Report Service
Port: 8202
Audit opinion, findings, and recommendations
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Audit Report Service", version="1.0.0")

class Finding(BaseModel):
    finding_id: str
    description: str
    impact: str
    severity: str
    recommendation: str

class AuditReportRequest(BaseModel):
    audit_id: str
    company_id: str
    fiscal_year: str
    opinion: str
    key_audit_matters: List[str]
    findings: List[Dict[str, Any]]
    material_weaknesses: List[str]
    going_concern_issues: bool

class AuditReportResponse(BaseModel):
    audit_id: str
    company_id: str
    fiscal_year: str
    opinion_type: str
    report_date: str
    key_audit_matters: List[str]
    findings_summary: List[Finding]
    material_weaknesses_disclosure: bool
    going_concern_disclosure: bool
    emphasis_of_matter: List[str]
    regulatory_filing_required: List[str]

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
    return {"status": "healthy", "service": "audit-report", "version": "1.0.0"}

@app.post("/generate", response_model=AuditReportResponse)
async def generate_audit_report(request: AuditReportRequest):
    logger.info("Generating audit report", audit=request.audit_id, company=request.company_id)

    findings_summary = [
        Finding(
            finding_id=f.get("id", ""),
            description=f.get("description", ""),
            impact=f.get("impact", ""),
            severity=f.get("severity", "medium"),
            recommendation=f.get("recommendation", "")
        ) for f in request.findings
    ]

    emphasis_matters = []
    if request.material_weaknesses:
        emphasis_matters.append("Material weaknesses in internal control")
    if request.going_concern_issues:
        emphasis_matters.append("Substantial doubt about going concern")

    opinion_type_map = {
        "unqualified": "Unqualified Opinion",
        "qualified": "Qualified Opinion",
        "adverse": "Adverse Opinion",
        "disclaimer": "Disclaimer of Opinion"
    }

    return AuditReportResponse(
        audit_id=request.audit_id,
        company_id=request.company_id,
        fiscal_year=request.fiscal_year,
        opinion_type=opinion_type_map.get(request.opinion, "Unqualified Opinion"),
        report_date="2024-06-30",
        key_audit_matters=request.key_audit_matters,
        findings_summary=findings_summary,
        material_weaknesses_disclosure=len(request.material_weaknesses) > 0,
        going_concern_disclosure=request.going_concern_issues,
        emphasis_of_matter=emphasis_matters if emphasis_matters else ["No emphasis of matter paragraphs"],
        regulatory_filing_required=["SEC Filing", "Stock Exchange Filing", "Tax Authorities"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8202)
