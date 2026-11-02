"""
Vimbai IFRS Reporting Service
IFRS financial reports, their figures and their disclosure notes.

Reports and notes live in Neo4j, scoped to the caller and the Book. The file may be
imported bare (bracket mounts, uvicorn main:app), so it bootstraps its own package
alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "ifrs_reporting_service" not in _sys.modules or not hasattr(_sys.modules.get("ifrs_reporting_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("ifrs_reporting_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["ifrs_reporting_service"] = _pkg
    _sys.modules["ifrs_reporting_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from ifrs_reporting_service import crud
from ifrs_reporting_service.dependencies import book_id_var, get_db_session, get_user_id
from ifrs_reporting_service.exceptions import ConflictError, NotFoundError
from ifrs_reporting_service.models import DisclosureNote, IFRSReport
from neo4j import AsyncSession

SERVICE_NAME = "ifrs-reporting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8268"))

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create an IFRS report."""
    valid_types = ["balance_sheet", "income_statement", "cash_flow", "equity_changes", "notes"]
    if report_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid report type. Must be one of {valid_types}")

    report = await crud.create_report(
        db_session,
        user_id,
        report_type=report_type,
        ifrs_standard=ifrs_standard,
        period=period,
        reporting_date=reporting_date,
        data=data,
    )
    logger.info("IFRS report created", report_id=report.id, type=report_type, standard=ifrs_standard)
    return report


@app.get("/reports", response_model=List[IFRSReport])
async def list_reports(
    report_type: Optional[str] = None,
    ifrs_standard: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List IFRS reports."""
    return await crud.list_reports(
        db_session, user_id, report_type=report_type, ifrs_standard=ifrs_standard, status=status
    )


@app.get("/reports/{report_id}", response_model=IFRSReport)
async def get_report(
    report_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific IFRS report."""
    try:
        return await crud.get_report(db_session, user_id, report_id)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Report not found")


@app.post("/reports/{report_id}/notes", response_model=DisclosureNote)
async def add_note(
    report_id: str,
    note_number: int,
    title: str,
    content: str = "",
    ifrs_reference: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a disclosure note to an IFRS report."""
    try:
        note = await crud.add_note(
            db_session,
            user_id,
            report_id,
            note_number=note_number,
            title=title,
            content=content,
            ifrs_reference=ifrs_reference,
        )
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Report not found")

    logger.info("Disclosure note added", report_id=report_id, note_number=note_number)
    return note


@app.get("/reports/{report_id}/notes", response_model=List[DisclosureNote])
async def list_notes(
    report_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List disclosure notes for a report."""
    return await crud.list_notes(db_session, user_id, report_id)


@app.put("/reports/{report_id}/approve")
async def approve_report(
    report_id: str,
    approved_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Approve and publish an IFRS report."""
    try:
        report = await crud.approve_report(db_session, user_id, report_id, approved_by)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Report not found")
    except ConflictError as exc:
        raise HTTPException(status_code=400, detail=str(exc))

    logger.info("IFRS report approved", report_id=report_id)
    return {"report_id": report_id, "status": "approved", "approved_by": report.approved_by}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
