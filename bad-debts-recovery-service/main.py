"""
Vimbai Bad Debts Recovery Service
Tracks and records recovery of previously written-off bad debts. Port: 8033

Debts and their recovery journals live in Neo4j, scoped to the caller and the
Book. The file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "bad_debts_recovery_service" not in _sys.modules or not hasattr(
    _sys.modules.get("bad_debts_recovery_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("bad_debts_recovery_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["bad_debts_recovery_service"] = _pkg
    _sys.modules["bad_debts_recovery_service"].__path__ = [_HERE]

import os
from datetime import datetime
from enum import Enum  # noqa: F401 - kept for callers importing from main
from typing import Any, Dict, Optional

import httpx
import structlog
from bad_debts_recovery_service import crud
from bad_debts_recovery_service.dependencies import book_id_var, get_db_session, get_user_id
from bad_debts_recovery_service.exceptions import ConflictError, NotFoundError
from bad_debts_recovery_service.models import RecoveryStatus
from fastapi import Depends, FastAPI, HTTPException, Query, Request, status
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "bad-debts-recovery-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8033"))
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

app = FastAPI(title="Vimbai Bad Debts Recovery Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.error("accounting_service_error", error=str(e))
        return {}


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
    except Exception as e:
        logger.error("audit_error", error=str(e))


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Bad debts recovery tracking"}


@app.post("/bad-debts/register")
async def register_bad_debt(
    debt_id: str,
    debtor_name: str,
    original_amount: float,
    written_off_amount: float,
    written_off_date: datetime,
    notes: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a bad debt for recovery tracking."""
    entry = await crud.register_debt(
        db_session,
        user_id,
        debt_id=debt_id,
        debtor_name=debtor_name,
        original_amount=original_amount,
        written_off_amount=written_off_amount,
        notes=notes,
    )
    await call_audit_service("CREATE", "bad_debt", debt_id, {"amount": written_off_amount})
    return entry


@app.post("/bad-debts/{debt_id}/recover")
async def record_recovery(
    debt_id: str,
    amount: float,
    recovery_date: Optional[datetime] = None,
    payment_method: Optional[str] = None,
    reference: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record recovery of a bad debt."""
    try:
        existing = await crud.get_debt(db_session, user_id, debt_id)
    except NotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bad debt record not found")
    if existing.status == RecoveryStatus.COMPLETED:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Recovery already completed")

    recovered_on = recovery_date or datetime.utcnow()

    # the journal the accounting service keeps, unchanged in shape
    journal_entry = {
        "date": recovered_on,
        "description": f"Recovery of bad debt from {existing.debtor_name}",
        "entries": [
            {"account_code": "1000", "description": "Cash/Bank", "debit": amount, "credit": 0},
            {
                "account_code": "1310",
                "description": "Provision for Doubtful Debts",
                "debit": amount if existing.recovered_amount == 0 else 0,
                "credit": 0,
            },
            {"account_code": "6400", "description": "Bad Debts Recovered", "debit": 0, "credit": amount},
        ],
        "reference": f"BAD-DBT-REC-{debt_id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    try:
        entry, recovery = await crud.record_recovery(
            db_session,
            user_id,
            debt_id=debt_id,
            amount=amount,
            recovery_date=recovered_on,
            payment_method=payment_method,
            reference=reference,
            journal_entry_id=result.get("id"),
        )
    except NotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bad debt record not found")
    except ConflictError:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Recovery already completed")

    await call_audit_service(
        "RECOVER", "bad_debt", debt_id, {"amount": amount, "total_recovered": entry.recovered_amount}
    )
    return {"entry": entry, "recovery": recovery}


@app.get("/bad-debts")
async def list_bad_debts(
    status_filter: Optional[RecoveryStatus] = Query(None, alias="status"),
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all bad debt records."""
    records = await crud.list_debts(db_session, user_id, status_filter)
    return {"records": records, "total": len(records)}


@app.get("/bad-debts/{debt_id}")
async def get_bad_debt(
    debt_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get bad debt details."""
    try:
        return await crud.get_debt(db_session, user_id, debt_id)
    except NotFoundError:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bad debt record not found")


@app.get("/recovery-summary")
async def get_recovery_summary(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get recovery summary statistics."""
    return crud.summarise(await crud.list_debts(db_session, user_id))


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
