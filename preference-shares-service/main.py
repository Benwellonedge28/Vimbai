# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "preference_shares_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Preference Shares Service
Manages preference share classes, dividends, and redemptions.

Record-keeping only: this service records preference share movements and
journal-entry references (user-owned, Book-scoped via X-User-Id /
X-Book-ID); it never moves money. Corrections use reversing entries.
"""

import os
from datetime import datetime
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from preference_shares_service import crud, models
from preference_shares_service.database import Neo4jConnector
from preference_shares_service.dependencies import book_id_var, get_db_session, get_user_id
from preference_shares_service.exceptions import ConflictError, NotFoundError, ValidationError

SERVICE_NAME = "preference-shares-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8051"))
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

app = FastAPI(title="Vimbai Preference Shares Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Preference shares management"}


@app.post("/classes/create")
async def create_share_class(
    name: str,
    company_id: str,
    nominal_value: float,
    issue_price: float,
    fixed_dividend_rate: float,
    dividend_type: str,
    participation_rights: str,
    liquidation_priority: int,
    redemption_terms: Optional[str] = None,
    conversion_terms: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a preference share class."""
    share_class = models.PreferenceShareClass(
        name=name,
        company_id=company_id,
        nominal_value=nominal_value,
        issue_price=issue_price,
        fixed_dividend_rate=fixed_dividend_rate,
        dividend_type=dividend_type,
        participation_rights=participation_rights,
        liquidation_priority=liquidation_priority,
        redemption_terms=redemption_terms,
        conversion_terms=conversion_terms,
    )
    return await crud.create_share_class(db_session, user_id, share_class)


@app.post("/classes/{share_class_id}/issue")
async def issue_preference_shares(
    share_class_id: str,
    shares_issued: int,
    issue_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Issue preference shares of a given class."""
    share_class = await crud.get_share_class(db_session, user_id, share_class_id)
    if not share_class:
        return {"error": "Share class not found"}
    share_class.shares_issued += shares_issued
    share_class.shares_outstanding += shares_issued
    await crud.save_share_class(db_session, user_id, share_class)
    proceeds = shares_issued * share_class.issue_price
    nominal = shares_issued * share_class.nominal_value
    premium = proceeds - nominal
    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {shares_issued} {share_class.name} preference shares",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": proceeds, "credit": 0},
            {"account_code": "3205", "description": "Preference Share Capital", "debit": 0, "credit": nominal},
            {"account_code": "3215", "description": "Preference Share Premium", "debit": 0, "credit": premium},
        ],
        "reference": f"PREF-ISS-{share_class_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    return share_class


@app.post("/classes/{share_class_id}/dividends/declare")
async def declare_preference_dividend(
    share_class_id: str,
    company_id: str,
    per_share_amount: float,
    total_shares: int,
    preference_arears: float,
    record_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Declare preference share dividend."""
    share_class = await crud.get_share_class(db_session, user_id, share_class_id)
    if not share_class:
        return {"error": "Share class not found"}
    dividend = models.PreferenceDividend(
        share_class_id=share_class_id,
        company_id=company_id,
        dividend_type="fixed",
        per_share_amount=per_share_amount,
        total_shares=total_shares,
        preference_arears=preference_arears,
        record_date=record_date,
    )
    dividend.total_dividend = (per_share_amount * total_shares) + preference_arears
    journal_entry = {
        "date": record_date,
        "description": f"Declaration of {share_class.name} preference dividend",
        "entries": [
            {
                "account_code": "3300",
                "description": "Retained Earnings",
                "debit": dividend.total_dividend,
                "credit": 0,
            },
            {
                "account_code": "2315",
                "description": "Preference Dividend Payable",
                "debit": 0,
                "credit": dividend.total_dividend,
            },
        ],
        "reference": f"PREF-DIV-{dividend.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.journal_entry_id = result.get("id")
    return await crud.create_dividend(db_session, user_id, dividend)


@app.post("/classes/{share_class_id}/dividends/{dividend_id}/pay")
async def pay_preference_dividend(
    share_class_id: str,
    dividend_id: str,
    payment_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record preference dividend payment (records only; moves no money)."""
    dividend = await crud.get_dividend(db_session, user_id, dividend_id)
    if not dividend:
        return {"error": "Dividend not found"}
    journal_entry = {
        "date": payment_date,
        "description": "Payment of preference dividend",
        "entries": [
            {
                "account_code": "2315",
                "description": "Preference Dividend Payable",
                "debit": dividend.total_dividend,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": dividend.total_dividend},
        ],
        "reference": f"PREF-DIV-PAY-{dividend_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.payment_date = payment_date
    dividend.status = "paid"
    await crud.save_dividend(db_session, user_id, dividend)
    return dividend


@app.post("/classes/{share_class_id}/redeem")
async def redeem_preference_shares(
    share_class_id: str,
    shares_redeemed: int,
    redemption_price: float,
    redemption_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Redeem preference shares."""
    share_class = await crud.get_share_class(db_session, user_id, share_class_id)
    if not share_class:
        return {"error": "Share class not found"}
    redemption = models.RedemptionEntry(
        share_class_id=share_class_id,
        shares_redeemed=shares_redeemed,
        redemption_price=redemption_price,
        redemption_date=redemption_date,
    )
    redemption.total_proceeds = shares_redeemed * redemption_price
    share_class.shares_outstanding -= shares_redeemed
    await crud.save_share_class(db_session, user_id, share_class)
    nominal = shares_redeemed * share_class.nominal_value
    journal_entry = {
        "date": redemption_date,
        "description": f"Redemption of {shares_redeemed} {share_class.name} preference shares",
        "entries": [
            {"account_code": "3205", "description": "Preference Share Capital", "debit": nominal, "credit": 0},
            {
                "account_code": "3220",
                "description": "Capital Redemption Reserve",
                "debit": redemption.total_proceeds - nominal,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": redemption.total_proceeds},
        ],
        "reference": f"PREF-RED-{redemption.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    redemption.journal_entry_id = result.get("id")
    return await crud.create_redemption(db_session, user_id, redemption)


@app.get("/classes")
async def list_share_classes(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List preference share classes."""
    result = await crud.list_share_classes(db_session, user_id)
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    return {"share_classes": result}


@app.get("/dividends")
async def list_dividends(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List preference dividends."""
    result = await crud.list_dividends(db_session, user_id)
    if company_id:
        result = [d for d in result if d.company_id == company_id]
    return {"dividends": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
