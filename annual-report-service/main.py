"""
Annual Report Service
Port: 8210
Annual report preparation and compliance checklist
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Annual Report Service", version="1.0.0")

class ReportSection(BaseModel):
    section_name: str
    required: bool
    completed: bool
    status: str

class AnnualReportRequest(BaseModel):
    company_id: str
    fiscal_year: str
    report_type: str
    jurisdiction: str
    sections_included: List[Dict[str, Any]]
    audit_opinion: str
    regulatory_filings: List[str]

class AnnualReportResponse(BaseModel):
    company_id: str
    fiscal_year: str
    report_status: str
    sections: List[ReportSection]
    completed_sections: int
    pending_sections: int
    regulatory_compliance: Dict[str, bool]
    filing_deadlines: Dict[str, str]
    outstanding_items: List[str]
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
    return {"status": "healthy", "service": "annual-report", "version": "1.0.0"}

@app.post("/prepare", response_model=AnnualReportResponse)
async def prepare_annual_report(request: AnnualReportRequest):
    logger.info("Preparing annual report", company=request.company_id, year=request.fiscal_year)

    sections = []
    completed = 0
    pending = 0

    for section in request.sections_included:
        status = section.get("status", "pending")
        if status == "completed":
            completed += 1
        else:
            pending += 1

        sections.append(ReportSection(
            section_name=section.get("name", ""),
            required=section.get("required", True),
            completed=status == "completed",
            status=status
        ))

    regulatory_filings = {
        "sec_filing": "SEC Filing" in request.regulatory_filings,
        "stock_exchange": "Stock Exchange" in request.regulatory_filings,
        "tax_return": "Tax Return" in request.regulatory_filings
    }

    outstanding = [s.section_name for s in sections if not s.completed and s.required]

    report_status = "Ready for publication" if pending == 0 else "In preparation"
    if request.audit_opinion != "unqualified":
        report_status += f" ({request.audit_opinion} opinion)"

    return AnnualReportResponse(
        company_id=request.company_id,
        fiscal_year=request.fiscal_year,
        report_status=report_status,
        sections=sections,
        completed_sections=completed,
        pending_sections=pending,
        regulatory_compliance=regulatory_filings,
        filing_deadlines={
            "annual_report": f"{request.fiscal_year}-04-30",
            "sec_filing": f"{request.fiscal_year}-03-31",
            "agm": f"{request.fiscal_year}-06-30"
        },
        outstanding_items=outstanding if outstanding else ["All required sections completed"],
        recommendations=[
            "Complete all pending sections before filing deadline",
            "Obtain board approval for annual report",
            "Ensure audit opinion is attached",
            "Submit regulatory filings on time"
        ] if pending > 0 else ["Proceed with report publication"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8210)
