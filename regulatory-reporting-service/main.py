"""
Regulatory Reporting Service
Port: 8276
Regulatory compliance reporting
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Regulatory Reporting Service", version="1.0.0")

class RegulatoryFiling(BaseModel):
    filing_id: str
    regulator: str
    filing_type: str
    due_date: str
    status: str

class RegulatoryReportingRequest(BaseModel):
    company_id: str
    filings: List[RegulatoryFiling]
    regulatory_metrics: Dict[str, float]

class RegulatoryReportingResponse(BaseModel):
    company_id: str
    report_date: str
    filing_summary: Dict[str, Any]
    compliance_status: Dict[str, Any]
    upcoming_deadlines: List[Dict[str, Any]]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "regulatory-reporting", "version": "1.0.0"}

@app.post("/analyze", response_model=RegulatoryReportingResponse)
async def analyze_regulatory_reporting(request: RegulatoryReportingRequest):
    logger.info("Analyzing regulatory reporting", company=request.company_id)

    on_time = sum(1 for f in request.filings if f.status == "Filed On Time")
    late = sum(1 for f in request.filings if f.status == "Late")
    pending = sum(1 for f in request.filings if f.status == "Pending")
    
    filing_summary = {
        "total_filings": len(request.filings),
        "filed_on_time": on_time,
        "late": late,
        "pending": pending,
        "compliance_rate": round(on_time / len(request.filings) * 100, 2) if request.filings else 0
    }
    
    compliance_status = {
        "regulators": list(set(f.regulator for f in request.filings)),
        "overall_status": "Compliant" if filing_summary["compliance_rate"] >= 95 else "At Risk",
        "critical_filings_overdue": late
    }
    
    upcoming_deadlines = [
        {"regulator": f.regulator, "type": f.filing_type, "due": f.due_date}
        for f in request.filings if f.status == "Pending"
    ][:5]
    
    recommendations = []
    if filing_summary["compliance_rate"] < 95:
        recommendations.append("Compliance rate below target - improve filing processes")
    if pending > 3:
        recommendations.append("Multiple pending filings - prioritize upcoming deadlines")

    return RegulatoryReportingResponse(
        company_id=request.company_id,
        report_date=datetime.now().isoformat(),
        filing_summary=filing_summary,
        compliance_status=compliance_status,
        upcoming_deadlines=upcoming_deadlines,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8276)
