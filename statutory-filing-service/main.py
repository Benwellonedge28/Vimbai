"""
Statutory Filing Service
Port: 8227
Regulatory filing requirements and compliance
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Statutory Filing Service", version="1.0.0")

class FilingRequirement(BaseModel):
    filing_type: str
    jurisdiction: str
    deadline: str
    status: str
    submitted: bool
    late_filing_penalty: float

class StatutoryFilingRequest(BaseModel):
    company_id: str
    fiscal_year: str
    jurisdictions: List[str]
    filings_required: List[Dict[str, Any]]
    previous_filings: List[Dict[str, Any]]

class StatutoryFilingResponse(BaseModel):
    company_id: str
    fiscal_year: str
    filing_requirements: List[FilingRequirement]
    upcoming_deadlines: List[Dict[str, str]]
    compliance_status: str
    outstanding_filings: List[str]
    late_filing_exposure: float
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
    return {"status": "healthy", "service": "statutory-filing", "version": "1.0.0"}

@app.post("/assess", response_model=StatutoryFilingResponse)
async def assess_statutory_filings(request: StatutoryFilingRequest):
    logger.info("Assessing statutory filings", company=request.company_id, year=request.fiscal_year)

    filing_requirements = []
    upcoming_deadlines = []
    outstanding = []
    late_exposure = 0.0

    for filing in request.filings_required:
        submitted = filing.get("submitted", False)
        status = "submitted" if submitted else "pending"
        penalty = filing.get("late_penalty", 0.0) if not submitted else 0.0

        late_exposure += penalty

        if not submitted:
            outstanding.append(f"{filing.get('type')} - {filing.get('jurisdiction')}")
            upcoming_deadlines.append({"type": filing.get("type"), "deadline": filing.get("deadline", "")})

        filing_requirements.append(FilingRequirement(
            filing_type=filing.get("type", ""),
            jurisdiction=filing.get("jurisdiction", ""),
            deadline=filing.get("deadline", ""),
            status=status,
            submitted=submitted,
            late_filing_penalty=penalty
        ))

    compliance_status = "compliant" if len(outstanding) == 0 else "pending_filings"

    return StatutoryFilingResponse(
        company_id=request.company_id,
        fiscal_year=request.fiscal_year,
        filing_requirements=filing_requirements,
        upcoming_deadlines=upcoming_deadlines,
        compliance_status=compliance_status,
        outstanding_filings=outstanding if outstanding else ["All required filings submitted"],
        late_filing_exposure=round(late_exposure, 2),
        recommendations=["Submit all outstanding filings before deadlines", "Review filing requirements for each jurisdiction", "Set up filing calendar reminders"] if outstanding else ["Maintain compliance tracking"]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8227)
