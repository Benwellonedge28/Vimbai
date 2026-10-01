"""Vimbai Treasury Compliance Service - Basel III / SOX compliance checks. Port: 8323

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "treasury_compliance_service" not in _sys.modules or not hasattr(
    _sys.modules.get("treasury_compliance_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("treasury_compliance_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["treasury_compliance_service"] = _pkg
    _sys.modules["treasury_compliance_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from treasury_compliance_service import crud, models
from treasury_compliance_service.dependencies import book_id_var, get_db_session, get_user_id
from treasury_compliance_service.exceptions import TreasuryComplianceError
from treasury_compliance_service.models import ComplianceStatus

SERVICE_NAME = "treasury-compliance-service"
PORT = int(os.getenv("PORT", "8323"))
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
app = FastAPI(title="Vimbai Treasury Compliance Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="treasury-compliance-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(TreasuryComplianceError)
async def _treasury_compliance_error(request: Request, exc: TreasuryComplianceError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.get("/checks/{company_id}")
async def get_compliance_checks(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's Book-visible checks (defaults are seeded on first access)."""
    checks = await crud.list_checks(db_session, user_id, company_id)
    compliant = sum(1 for c in checks if c.status == ComplianceStatus.COMPLIANT)
    return {
        "company_id": company_id,
        "checks": checks,
        "total": len(checks),
        "compliant": compliant,
        "compliance_rate": compliant / max(1, len(checks)),
    }


@app.put("/checks/{check_id}/status")
async def update_check_status(
    check_id: str,
    status: ComplianceStatus,
    remediation: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a check's status (and optional remediation note); cross-scope updates 404."""
    check = await crud.update_check_status(db_session, user_id, check_id, status, remediation)
    logger.info("check_status_updated", check_id=check_id, status=status.value)
    return {"check_id": check_id, "status": status.value}


@app.get("/report/{company_id}")
async def compliance_report(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Compliance report over the caller's Book-visible checks (never seeds)."""
    checks = await crud.list_checks(db_session, user_id, company_id, seed=False)
    if not checks:
        return {
            "company_id": company_id,
            "compliance_rate": 1.0,
            "findings": [],
            "recommendations": ["Run compliance checks first"],
        }
    findings = [c for c in checks if c.status in (ComplianceStatus.WARNING, ComplianceStatus.NON_COMPLIANT)]
    return {
        "company_id": company_id,
        "compliance_rate": sum(1 for c in checks if c.status == ComplianceStatus.COMPLIANT) / len(checks),
        "total_findings": len(findings),
        "findings": findings,
        "recommendations": [f.remediation for f in findings if f.remediation],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
