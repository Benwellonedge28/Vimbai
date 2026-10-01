"""Vimbai Operational Audit Service - audit engagements, findings and remediation. Port: 8352

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "operational_audit_service" not in _sys.modules or not hasattr(
    _sys.modules.get("operational_audit_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("operational_audit_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["operational_audit_service"] = _pkg
    _sys.modules["operational_audit_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from operational_audit_service import crud, models
from operational_audit_service.dependencies import book_id_var, get_db_session, get_user_id
from operational_audit_service.exceptions import OperationalAuditError

SERVICE_NAME = "operational-audit-service"
PORT = int(os.getenv("PORT", "8352"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Operational Audit Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(OperationalAuditError)
async def _operational_error(request: Request, exc: OperationalAuditError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/engagements", response_model=models.AuditEngagement)
async def create_engagement(
    engagement: models.AuditEngagementCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_engagement(db_session, user_id, engagement)
    logger.info("engagement_created", company_id=item.company_id, type=item.audit_type)
    return item


@app.get("/engagements/{company_id}")
async def get_engagements(
    company_id: str,
    status_filter: str = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    engagements = await crud.list_engagements(db_session, user_id, company_id, status_filter or "")
    return {"company_id": company_id, "engagements": engagements, "total": len(engagements)}


@app.put("/engagements/{engagement_id}/status")
async def update_status(
    engagement_id: str,
    status: models.AuditStatus,
    summary: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.update_status(db_session, user_id, engagement_id, status, summary)
    except OperationalAuditError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.post("/engagements/{engagement_id}/findings")
async def add_finding(
    engagement_id: str,
    finding: models.AuditFindingCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.add_finding(db_session, user_id, engagement_id, finding)
    except OperationalAuditError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.put("/findings/{finding_id}/remediate")
async def remediate_finding(
    finding_id: str,
    remediation_note: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.remediate_finding(db_session, user_id, finding_id, remediation_note)
    except OperationalAuditError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/report/{engagement_id}")
async def audit_report(
    engagement_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.audit_report(db_session, user_id, engagement_id)
    except OperationalAuditError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
