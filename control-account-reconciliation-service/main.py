"""
Vimbai Control Account Reconciliation Service
Control account breaks, resolutions and reconciliation reports.

Errors and reports live in Neo4j, scoped to the caller and the Book. The file may be
imported bare (bracket mounts, uvicorn main:app), so it bootstraps its own package
alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "control_account_reconciliation_service" not in _sys.modules or not hasattr(
    _sys.modules.get("control_account_reconciliation_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "control_account_reconciliation_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["control_account_reconciliation_service"] = _pkg
    _sys.modules["control_account_reconciliation_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, Optional

import httpx
import structlog
from control_account_reconciliation_service import crud
from control_account_reconciliation_service.dependencies import book_id_var, get_db_session, get_user_id
from control_account_reconciliation_service.exceptions import NotFoundError
from control_account_reconciliation_service.models import (
    ControlAccountType,
    ErrorType,
    ReconciliationError,
)
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "control-account-reconciliation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8040"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Control Account Reconciliation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(
                f"{AUDIT_SERVICE_URL}/audit",
                json={
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )
    except Exception:
        pass


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Control account reconciliation and error handling",
    }


@app.post("/errors/report")
async def report_error(
    data: ReconciliationError,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Report a reconciliation error."""
    payload = data.model_dump()
    for generated in ("id", "created_at", "variance", "user_id", "book_id"):
        payload.pop(generated, None)
    error = await crud.create_error(db_session, user_id, payload)
    await call_audit_service("CREATE", "error", error.id, {"type": error.error_type, "amount": error.variance})
    return error


@app.get("/errors")
async def list_errors(
    status: Optional[str] = None,
    error_type: Optional[ErrorType] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all reconciliation errors."""
    errors = await crud.list_errors(db_session, user_id, status=status, error_type=error_type)
    return {"errors": errors, "count": len(errors)}


@app.post("/errors/{error_id}/resolve")
async def resolve_error(
    error_id: str,
    resolution: str,
    resolved_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Resolve a reconciliation error."""
    try:
        error = await crud.resolve_error(db_session, user_id, error_id, resolution, resolved_by)
    except NotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Error not found")

    await call_audit_service("RESOLVE", "error", error_id, {"resolution": resolution})
    return error


@app.post("/reconcile/{account_type}")
async def reconcile_control_account(
    account_type: ControlAccountType,
    as_of_date: datetime,
    control_balance: float,
    ledger_total: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Perform reconciliation for a control account."""
    report = await crud.reconcile(db_session, user_id, account_type, as_of_date, control_balance, ledger_total)
    await call_audit_service("RECONCILE", "report", report.id, {"difference": report.difference})
    return report


@app.get("/reports")
async def list_reports(
    limit: int = 10,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List reconciliation reports."""
    return {"reports": await crud.list_reports(db_session, user_id, limit)}


@app.get("/summary")
async def get_reconciliation_summary(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get summary of all reconciliation issues."""
    return await crud.summary(db_session, user_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
