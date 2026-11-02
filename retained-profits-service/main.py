"""
Vimbai Retained Profits Service
Profit appropriations, interim dividends and prior year adjustments. Port: 8036

Persistence is Neo4j, scoped to the caller and the Book. The file may be imported
bare (bracket mounts, uvicorn main:app), so it bootstraps its own package alias
first.

FIXED LATENT BUG: every journal posted to the accounting service carried datetime
objects in its body, so the payload never serialised, the exception was swallowed
and journal_entry_id always came back empty on all three records. The dates are
serialised now.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "retained_profits_service" not in _sys.modules or not hasattr(
    _sys.modules.get("retained_profits_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("retained_profits_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["retained_profits_service"] = _pkg
    _sys.modules["retained_profits_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from retained_profits_service import crud
from retained_profits_service.dependencies import book_id_var, get_db_session, get_user_id
from retained_profits_service.exceptions import NotFoundError

SERVICE_NAME = "retained-profits-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8054"))
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

app = FastAPI(title="Vimbai Retained Profits Service", version=SERVICE_VERSION, docs_url="/docs")
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
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Retained profits management"}


@app.post("/appropriations/create")
async def create_appropriation(
    company_id: str,
    financial_year: str,
    period_start: datetime,
    period_end: datetime,
    net_profit: float,
    opening_retained_earnings: float,
    appropriations: Dict[str, float],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create profit appropriation statement."""
    entries = [{"account_code": "3300", "description": "Retained Earnings", "debit": net_profit, "credit": 0}]
    for _alloc_type, amount in appropriations.items():
        entries.append({"account_code": "3300", "description": "Retained Earnings", "debit": 0, "credit": amount})

    journal_entry = {
        "date": period_end.isoformat(),
        "description": f"Profit appropriation for {financial_year}",
        "entries": entries,
        "reference": "APPROP-PENDING",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    appropriation = await crud.create_appropriation(
        db_session,
        user_id,
        company_id=company_id,
        financial_year=financial_year,
        period_start=period_start,
        period_end=period_end,
        net_profit=net_profit,
        opening_retained_earnings=opening_retained_earnings,
        appropriations=appropriations,
        journal_entry_id=result.get("id"),
    )
    return appropriation


@app.post("/interim-dividends/declare")
async def declare_interim_dividend(
    company_id: str,
    amount: float,
    per_share_amount: float,
    total_shares: int,
    declaration_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Declare interim dividend."""
    journal_entry = {
        "date": declaration_date.isoformat(),
        "description": "Declaration of interim dividend",
        "entries": [
            {"account_code": "3300", "description": "Retained Earnings", "debit": amount, "credit": 0},
            {"account_code": "2310", "description": "Dividend Payable", "debit": 0, "credit": amount},
        ],
        "reference": "INTERIM-DIV-PENDING",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    return await crud.declare_dividend(
        db_session,
        user_id,
        company_id=company_id,
        amount=amount,
        per_share_amount=per_share_amount,
        total_shares=total_shares,
        declaration_date=declaration_date,
        journal_entry_id=result.get("id"),
    )


@app.post("/interim-dividends/{dividend_id}/pay")
async def pay_interim_dividend(
    dividend_id: str,
    payment_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Pay interim dividend."""
    try:
        dividend = await crud.pay_dividend(db_session, user_id, dividend_id, payment_date)
    except NotFoundError:
        # the original answers 200 with an error body here, not a 404
        return {"error": "Dividend not found"}

    journal_entry = {
        "date": payment_date.isoformat(),
        "description": "Payment of interim dividend",
        "entries": [
            {"account_code": "2310", "description": "Dividend Payable", "debit": dividend.amount, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": dividend.amount},
        ],
        "reference": f"INTERIM-DIV-PAY-{dividend_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    return dividend


@app.post("/adjustments/create")
async def create_prior_year_adjustment(
    company_id: str,
    adjustment_type: str,
    description: str,
    amount: float,
    affected_year: str,
    adjustment_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create prior year adjustment."""
    adjustment_date = adjustment_date or datetime.utcnow()

    journal_entry = {
        "date": adjustment_date.isoformat(),
        "description": f"Prior year adjustment: {description}",
        "entries": [
            {
                "account_code": "3300",
                "description": "Retained Earnings",
                "debit": amount if amount > 0 else 0,
                "credit": abs(amount) if amount < 0 else 0,
            },
            {
                "account_code": "1100",
                "description": "Trade Receivables",
                "debit": abs(amount) if amount < 0 else 0,
                "credit": amount if amount > 0 else 0,
            },
        ],
        "reference": "PYA-PENDING",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    return await crud.create_adjustment(
        db_session,
        user_id,
        company_id=company_id,
        adjustment_type=adjustment_type,
        description=description,
        amount=amount,
        affected_year=affected_year,
        adjustment_date=adjustment_date,
        journal_entry_id=result.get("id"),
    )


@app.get("/appropriations")
async def list_appropriations(
    company_id: Optional[str] = None,
    financial_year: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List profit appropriations."""
    return {"appropriations": await crud.list_appropriations(db_session, user_id, company_id, financial_year)}


@app.get("/interim-dividends")
async def list_interim_dividends(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List interim dividends."""
    return {"interim_dividends": await crud.list_dividends(db_session, user_id, company_id)}


@app.get("/adjustments")
async def list_adjustments(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List prior year adjustments."""
    return {"adjustments": await crud.list_adjustments(db_session, user_id, company_id)}


@app.get("/summary/{company_id}")
async def get_retained_earnings_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get retained earnings summary."""
    return await crud.summarise(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
