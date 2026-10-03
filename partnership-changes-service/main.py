"""
Vimbai Partnership Changes Service
Manages admission, retirement, death, and insolvency of partners.

Records persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway). All
lookups and settlements are scoped to the caller's own Book-visible
records. Original status codes, totals math, and fail-soft accounting
side-calls are preserved.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "partnership_changes_service" not in _sys.modules or not hasattr(
    _sys.modules.get("partnership_changes_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("partnership_changes_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["partnership_changes_service"] = _pkg
    _sys.modules["partnership_changes_service"].__path__ = [_HERE]

import httpx
import structlog
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from partnership_changes_service import crud
from partnership_changes_service.database import Neo4jConnector
from partnership_changes_service.dependencies import book_id_var, get_db_session, get_user_id
from partnership_changes_service.exceptions import PartnershipChangesServiceError
from partnership_changes_service.models import AdmissionDetails, ChangeType, PartnerChange

SERVICE_NAME = "partnership-changes-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8043"))
AUDIT_SERVICE_URL = _os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = _os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Partnership Changes Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(PartnershipChangesServiceError)
async def _pc_error(request: Request, exc: PartnershipChangesServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "description": "Admission, retirement, death, insolvency changes"}


@app.post("/changes/retirement")
async def record_retirement(
    partnership_id: str,
    partner_id: str,
    partner_name: str,
    effective_date: datetime,
    capital_balance: float,
    current_account_balance: float,
    goodwill_amount: float = 0,
    payment_method: str = "cash",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record partner retirement."""
    total_payable = capital_balance + current_account_balance + goodwill_amount

    change = PartnerChange(
        partnership_id=partnership_id,
        change_type=ChangeType.RETIREMENT,
        partner_id=partner_id,
        partner_name=partner_name,
        effective_date=effective_date,
        capital_balance=capital_balance,
        current_account_balance=current_account_balance,
        total_payable=total_payable,
        goodwill_amount=goodwill_amount,
        payment_method=payment_method,
    )
    journal_entry = {
        "date": effective_date,
        "description": f"Retirement of partner: {partner_name}",
        "entries": [
            {"account_code": "3000", "description": "Partner Capital", "debit": capital_balance, "credit": 0},
            {
                "account_code": "3100",
                "description": "Partner Current Account",
                "debit": current_account_balance,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Cash/Bank", "debit": 0, "credit": total_payable},
        ],
        "reference": f"RET-{partner_id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    change.journal_entry_id = result.get("id")
    return await crud.create_change(db_session, user_id, change)


@app.post("/changes/admission")
async def record_admission(
    data: AdmissionDetails,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record new partner admission."""
    data.id = str(uuid.uuid4())
    data.admission_date = data.admission_date or datetime.now(timezone.utc)

    journal_entries = []

    # Capital contribution
    if data.capital_contribution > 0:
        journal_entries.append(
            {
                "date": data.admission_date,
                "description": f"Capital contribution by {data.new_partner_name}",
                "entries": [
                    {
                        "account_code": "1000",
                        "description": "Cash/Bank",
                        "debit": data.capital_contribution,
                        "credit": 0,
                    },
                    {
                        "account_code": "3000",
                        "description": f"Partner Capital - {data.new_partner_name}",
                        "debit": 0,
                        "credit": data.capital_contribution,
                    },
                ],
                "reference": f"ADM-{data.new_partner_id[:8]}",
            }
        )

    # Goodwill premium
    if data.goodwill_paid > 0:
        journal_entries.append(
            {
                "date": data.admission_date,
                "description": "Goodwill premium",
                "entries": [
                    {"account_code": "1000", "description": "Cash/Bank", "debit": data.goodwill_paid, "credit": 0},
                    {"account_code": "1500", "description": "Goodwill", "debit": 0, "credit": data.goodwill_paid},
                ],
                "reference": f"GW-{data.new_partner_id[:8]}",
            }
        )

    for entry in journal_entries:
        result = await call_accounting_service("POST", "/journal-entries", entry)
        data.journal_entry_ids.append(result.get("id", ""))

    return await crud.create_admission(db_session, user_id, data)


@app.post("/changes/death")
async def record_death(
    partnership_id: str,
    partner_id: str,
    partner_name: str,
    effective_date: datetime,
    capital_balance: float,
    current_account_balance: float,
    executor_name: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record partner death."""
    total_payable = capital_balance + current_account_balance

    change = PartnerChange(
        partnership_id=partnership_id,
        change_type=ChangeType.DEATH,
        partner_id=partner_id,
        partner_name=partner_name,
        effective_date=effective_date,
        capital_balance=capital_balance,
        current_account_balance=current_account_balance,
        total_payable=total_payable,
        notes=f"Payable to executor: {executor_name}",
    )
    return await crud.create_change(db_session, user_id, change)


@app.get("/changes")
async def list_changes(
    change_type: Optional[ChangeType] = None,
    partnership_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's partner changes."""
    result = await crud.list_changes(db_session, user_id)
    if change_type:
        result = [c for c in result if c.change_type == change_type]
    if partnership_id:
        result = [c for c in result if c.partnership_id == partnership_id]
    return {"changes": result, "count": len(result)}


@app.post("/changes/{change_id}/settle")
async def settle_change(
    change_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Mark the caller's change as settled."""
    change = await crud.get_change(db_session, user_id, change_id)
    if not change:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Change not found")
    await crud.settle_change(db_session, user_id, change_id)
    change.settlement_status = "settled"
    return change


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
