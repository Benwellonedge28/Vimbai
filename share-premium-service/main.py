# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "share_premium_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Share Premium Service
Records share premium received, utilized, and adjusted.

Record-keeping only: this service records share premium movements and
journal-entry references (user-owned, Book-scoped via X-User-Id /
X-Book-ID); it never moves money. Corrections use reversing entries.
"""

import os
from datetime import datetime, timezone
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from share_premium_service import crud, models
from share_premium_service.database import Neo4jConnector
from share_premium_service.dependencies import book_id_var, get_db_session, get_user_id
from share_premium_service.exceptions import ConflictError, NotFoundError, ValidationError

SERVICE_NAME = "share-premium-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8052"))
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

app = FastAPI(title="Vimbai Share Premium Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Capture the Book context for the duration of the request."""
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


@app.on_event("startup")
async def startup():
    Neo4jConnector.configure(
        os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        os.getenv("NEO4J_USER", "neo4j"),
        os.getenv("NEO4J_PASSWORD", "password"),
    )


@app.on_event("shutdown")
async def shutdown():
    await Neo4jConnector.close_driver()


@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request: Request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ConflictError)
async def conflict_exception_handler(request: Request, exc: ConflictError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ValidationError)
async def validation_exception_handler(request: Request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Share premium management"}


@app.post("/entries/record")
async def record_share_premium(
    company_id: str,
    entry_type: str,
    shares_issued: int,
    nominal_value: float,
    issue_price: float,
    share_class: str,
    reference_id: str,
    entry_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record share premium from share issuance."""
    if entry_date is None:
        entry_date = datetime.now(timezone.utc)
    premium_amount = (issue_price - nominal_value) * shares_issued
    entry = models.SharePremiumEntry(
        company_id=company_id,
        entry_type=entry_type,
        shares_issued=shares_issued,
        nominal_value=nominal_value,
        issue_price=issue_price,
        premium_amount=premium_amount,
        share_class=share_class,
        reference_id=reference_id,
        entry_date=entry_date,
    )
    journal_entry = {
        "date": entry_date,
        "description": f"Share premium from issue of {shares_issued} {share_class} shares",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": shares_issued * issue_price, "credit": 0},
            {
                "account_code": "3200",
                "description": "Share Capital",
                "debit": 0,
                "credit": shares_issued * nominal_value,
            },
            {"account_code": "3210", "description": "Share Premium", "debit": 0, "credit": premium_amount},
        ],
        "reference": f"SP-{entry.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    entry.journal_entry_id = result.get("id")
    return await crud.create_entry(db_session, user_id, entry)


@app.post("/utilizations/record")
async def utilize_premium(
    company_id: str,
    amount: float,
    utilization_type: str,
    description: str,
    utilization_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Utilize share premium account."""
    if utilization_date is None:
        utilization_date = datetime.now(timezone.utc)
    utilization = models.PremiumUtilization(
        company_id=company_id,
        amount=amount,
        utilization_type=utilization_type,
        description=description,
        utilization_date=utilization_date,
    )
    if utilization_type == "bonus_issue":
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium utilized for bonus issue: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}",
        }
    elif utilization_type == "write_off":
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium written off: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "3300", "description": "Retained Earnings", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}",
        }
    elif utilization_type == "merger_expenses":
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium for merger expenses: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "4100", "description": "Administrative Expenses", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}",
        }
    else:  # legal_costs
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium for legal costs: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "4100", "description": "Legal Expenses", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}",
        }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    utilization.journal_entry_id = result.get("id")
    return await crud.create_utilization(db_session, user_id, utilization)


@app.post("/adjustments/create")
async def adjust_premium(
    company_id: str,
    adjustment_type: str,
    original_amount: float,
    adjustment_amount: float,
    description: str,
    adjustment_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Adjust share premium (correction or reclassification)."""
    if adjustment_date is None:
        adjustment_date = datetime.now(timezone.utc)
    adjustment = models.PremiumAdjustment(
        company_id=company_id,
        adjustment_type=adjustment_type,
        original_amount=original_amount,
        adjustment_amount=adjustment_amount,
        description=description,
        adjustment_date=adjustment_date,
    )
    journal_entry = {
        "date": adjustment_date,
        "description": f"Share premium adjustment: {description}",
        "entries": [
            {
                "account_code": "3210",
                "description": "Share Premium",
                "debit": abs(adjustment_amount) if adjustment_amount < 0 else 0,
                "credit": abs(adjustment_amount) if adjustment_amount > 0 else 0,
            },
            {
                "account_code": "3300",
                "description": "Retained Earnings",
                "debit": abs(adjustment_amount) if adjustment_amount > 0 else 0,
                "credit": abs(adjustment_amount) if adjustment_amount < 0 else 0,
            },
        ],
        "reference": f"SP-ADJ-{adjustment.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    adjustment.journal_entry_id = result.get("id")
    return await crud.create_adjustment(db_session, user_id, adjustment)


@app.get("/entries")
async def list_entries(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List share premium entries."""
    result = await crud.list_entries(db_session, user_id)
    if company_id:
        result = [e for e in result if e.company_id == company_id]
    return {"entries": result}


@app.get("/utilizations")
async def list_utilizations(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List premium utilizations."""
    result = await crud.list_utilizations(db_session, user_id)
    if company_id:
        result = [u for u in result if u.company_id == company_id]
    return {"utilizations": result}


@app.get("/summary/{company_id}")
async def get_premium_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get share premium summary."""
    entries = await crud.list_entries(db_session, user_id)
    utilizations = await crud.list_utilizations(db_session, user_id)
    adjustments = await crud.list_adjustments(db_session, user_id)
    company_entries = [e for e in entries if e.company_id == company_id]
    company_utilizations = [u for u in utilizations if u.company_id == company_id]
    company_adjustments = [a for a in adjustments if a.company_id == company_id]
    total_received = sum(e.premium_amount for e in company_entries)
    total_utilized = sum(u.amount for u in company_utilizations)
    total_adjusted = sum(a.adjustment_amount for a in company_adjustments)
    return {
        "company_id": company_id,
        "total_premium_received": total_received,
        "total_utilized": total_utilized,
        "total_adjusted": total_adjusted,
        "current_balance": total_received - total_utilized + total_adjusted,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
