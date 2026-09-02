"""
Vimbai IFRS Reporting Service
Generates IFRS-compliant financial statements and disclosure notes.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "ifrs-reporting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8430"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai IFRS Reporting Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class IFRSReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    report_type: str  # balance_sheet, income_statement, cash_flow, equity_changes, notes
    ifrs_standard: str  # IAS1, IFRS16, IFRS15, IFRS9, IAS36, etc.
    period: str  # YYYY-MM or YYYY
    reporting_date: datetime
    data: Dict[str, Any] = {}
    disclosures: List[Dict[str, Any]] = []
    status: str = "draft"  # draft, reviewed, approved, published
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    approved_by: str = ""


class DisclosureNote(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    report_id: str
    note_number: int
    title: str
    content: str = ""
    ifrs_reference: str = ""


reports: List[IFRSReport] = []
notes: List[DisclosureNote] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/reports", response_model=IFRSReport)
async def create_report(
    report_type: str,
    ifrs_standard: str,
    period: str,
    reporting_date: datetime,
    data: Dict[str, Any] = {},
):
    """Create an IFRS report."""
    valid_types = ["balance_sheet", "income_statement", "cash_flow", "equity_changes", "notes"]
    if report_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid report type. Must be one of {valid_types}")

    report = IFRSReport(
        report_type=report_type,
        ifrs_standard=ifrs_standard,
        period=period,
        reporting_date=reporting_date,
        data=data,
    )
    reports.append(report)
    logger.info("IFRS report created", report_id=report.id, type=report_type, standard=ifrs_standard)
    return report


@app.get("/reports", response_model=List[IFRSReport])
async def list_reports(
    report_type: Optional[str] = None, ifrs_standard: Optional[str] = None, status: Optional[str] = None
):
    """List IFRS reports."""
    result = reports
    if report_type:
        result = [r for r in result if r.report_type == report_type]
    if ifrs_standard:
        result = [r for r in result if r.ifrs_standard == ifrs_standard]
    if status:
        result = [r for r in result if r.status == status]
    return result


@app.get("/reports/{report_id}", response_model=IFRSReport)
async def get_report(report_id: str):
    """Get a specific IFRS report."""
    report = next((r for r in reports if r.id == report_id), None)
    if not report:
        raise HTTPException(status_code=404, detail="Report not found")
    return report


@app.post("/reports/{report_id}/notes", response_model=DisclosureNote)
async def add_note(report_id: str, note_number: int, title: str, content: str = "", ifrs_reference: str = ""):
    """Add a disclosure note to an IFRS report."""
    report = next((r for r in reports if r.id == report_id), None)
    if not report:
        raise HTTPException(status_code=404, detail="Report not found")

    note = DisclosureNote(
        report_id=report_id,
        note_number=note_number,
        title=title,
        content=content,
        ifrs_reference=ifrs_reference,
    )
    notes.append(note)
    report.disclosures.append({"note_number": note_number, "title": title})
    logger.info("Disclosure note added", report_id=report_id, note_number=note_number)
    return note


@app.get("/reports/{report_id}/notes", response_model=List[DisclosureNote])
async def list_notes(report_id: str):
    """List disclosure notes for a report."""
    return [n for n in notes if n.report_id == report_id]


@app.put("/reports/{report_id}/approve")
async def approve_report(report_id: str, approved_by: str):
    """Approve and publish an IFRS report."""
    report = next((r for r in reports if r.id == report_id), None)
    if not report:
        raise HTTPException(status_code=404, detail="Report not found")
    if report.status not in ("draft", "reviewed"):
        raise HTTPException(status_code=400, detail=f"Report cannot be approved from {report.status} state")

    report.status = "approved"
    report.approved_by = approved_by
    logger.info("IFRS report approved", report_id=report_id)
    return {"report_id": report_id, "status": "approved", "approved_by": approved_by}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
