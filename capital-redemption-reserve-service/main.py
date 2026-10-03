"""
Vimbai Capital Redemption Reserve Service
Handles capital redemption reserve (CRR) creation and utilization.

Records persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway). All
listings and the balance summary are scoped to the caller's own
Book-visible records. Reserve math, journal side-calls, and response
shapes are preserved exactly.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import Any, Dict, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "capital_redemption_reserve_service" not in _sys.modules or not hasattr(
    _sys.modules.get("capital_redemption_reserve_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "capital_redemption_reserve_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["capital_redemption_reserve_service"] = _pkg
    _sys.modules["capital_redemption_reserve_service"].__path__ = [_HERE]

import httpx
import structlog
from capital_redemption_reserve_service import crud
from capital_redemption_reserve_service.database import Neo4jConnector
from capital_redemption_reserve_service.dependencies import book_id_var, get_db_session, get_user_id
from capital_redemption_reserve_service.exceptions import CapitalRedemptionReserveServiceError
from capital_redemption_reserve_service.models import CRRCreation, CRRUtilization, RedemptionTransaction
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "capital-redemption-reserve-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8063"))
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

app = FastAPI(title="Vimbai Capital Redemption Reserve Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(CapitalRedemptionReserveServiceError)
async def _crr_error(request: Request, exc: CapitalRedemptionReserveServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Capital redemption reserve management"}


@app.post("/redemptions/record")
async def record_redemption(
    company_id: str,
    share_class: str,
    shares_redeemed: int,
    redemption_price: float,
    nominal_value: float,
    redemption_date: datetime,
    source_account: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record share redemption creating CRR."""
    transaction = RedemptionTransaction(
        company_id=company_id,
        share_class=share_class,
        shares_redeemed=shares_redeemed,
        redemption_price=redemption_price,
        nominal_value=nominal_value,
        redemption_date=redemption_date,
        source_account=source_account,
    )
    transaction.total_proceeds = shares_redeemed * redemption_price
    transaction.redemption_reserve_amount = transaction.total_proceeds - (shares_redeemed * nominal_value)

    capital_account = "3205" if share_class == "preference" else "3200"

    journal_entry = {
        "date": redemption_date,
        "description": f"Redemption of {shares_redeemed} {share_class} shares - CRR created",
        "entries": [
            {
                "account_code": capital_account,
                "description": f"{share_class.title()} Share Capital",
                "debit": shares_redeemed * nominal_value,
                "credit": 0,
            },
            {
                "account_code": "3220",
                "description": "Capital Redemption Reserve",
                "debit": transaction.redemption_reserve_amount,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": transaction.total_proceeds},
        ],
        "reference": f"CRR-RED-{transaction.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    transaction.journal_entry_id = result.get("id")

    return await crud.create_redemption(db_session, user_id, transaction)


@app.post("/creations/create")
async def create_crr(
    company_id: str,
    amount: float,
    source: str,
    description: str,
    creation_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Manually create CRR (e.g., from capital reduction)."""
    if creation_date is None:
        creation_date = datetime.now(timezone.utc)

    crr = CRRCreation(
        company_id=company_id, amount=amount, source=source, description=description, creation_date=creation_date
    )

    journal_entry = {
        "date": creation_date,
        "description": f"Creation of Capital Redemption Reserve: {description}",
        "entries": [
            {"account_code": "3300", "description": "Retained Earnings / P&L", "debit": amount, "credit": 0},
            {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": 0, "credit": amount},
        ],
        "reference": f"CRR-CREATE-{crr.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    crr.journal_entry_id = result.get("id")

    return await crud.create_creation(db_session, user_id, crr)


@app.post("/utilizations/record")
async def utilize_crr(
    company_id: str,
    amount: float,
    utilization_type: str,
    description: str,
    utilization_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Utilize CRR (e.g., for bonus issue)."""
    if utilization_date is None:
        utilization_date = datetime.now(timezone.utc)

    utilization = CRRUtilization(
        company_id=company_id,
        amount=amount,
        utilization_type=utilization_type,
        description=description,
        utilization_date=utilization_date,
    )

    if utilization_type == "bonus_issue":
        journal_entry = {
            "date": utilization_date,
            "description": f"CRR utilized for bonus issue: {description}",
            "entries": [
                {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": amount, "credit": 0},
                {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": amount},
            ],
            "reference": f"CRR-UTIL-{utilization.id[:8]}",
        }
    elif utilization_type == "write_off":
        journal_entry = {
            "date": utilization_date,
            "description": f"CRR written off: {description}",
            "entries": [
                {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": amount, "credit": 0},
                {"account_code": "3300", "description": "Retained Earnings", "debit": 0, "credit": amount},
            ],
            "reference": f"CRR-UTIL-{utilization.id[:8]}",
        }
    else:
        journal_entry = {
            "date": utilization_date,
            "description": f"CRR transferred: {description}",
            "entries": [
                {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": amount, "credit": 0},
                {"account_code": "3310", "description": "General Reserve", "debit": 0, "credit": amount},
            ],
            "reference": f"CRR-UTIL-{utilization.id[:8]}",
        }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    utilization.journal_entry_id = result.get("id")

    return await crud.create_utilization(db_session, user_id, utilization)


@app.get("/redemptions")
async def list_redemptions(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List redemption transactions."""
    result = await crud.list_redemptions(db_session, user_id)
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    return {"redemptions": result}


@app.get("/creations")
async def list_creations(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List CRR creations."""
    result = await crud.list_creations(db_session, user_id)
    if company_id:
        result = [c for c in result if c.company_id == company_id]
    return {"creations": result}


@app.get("/utilizations")
async def list_utilizations(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List CRR utilizations."""
    result = await crud.list_utilizations(db_session, user_id)
    if company_id:
        result = [u for u in result if u.company_id == company_id]
    return {"utilizations": result}


@app.get("/summary/{company_id}")
async def get_crr_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get CRR balance summary."""
    company_redemptions = [r for r in await crud.list_redemptions(db_session, user_id) if r.company_id == company_id]
    company_creations = [c for c in await crud.list_creations(db_session, user_id) if c.company_id == company_id]
    company_utilizations = [u for u in await crud.list_utilizations(db_session, user_id) if u.company_id == company_id]

    total_created = sum(r.redemption_reserve_amount for r in company_redemptions) + sum(
        c.amount for c in company_creations
    )
    total_utilized = sum(u.amount for u in company_utilizations)

    return {
        "company_id": company_id,
        "total_created": total_created,
        "total_utilized": total_utilized,
        "current_balance": total_created - total_utilized,
        "transaction_count": len(company_redemptions) + len(company_creations) + len(company_utilizations),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
