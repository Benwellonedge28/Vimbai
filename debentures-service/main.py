"""
Vimbai Debentures Service
Debenture classes, issues, interest, and redemptions.

Records persist in Neo4j, stamped with the caller (X-User-Id) and the Book
context (X-Book-ID, verified upstream by the API gateway). Every lookup is
scoped to the caller's own Book-visible records. Original status codes and
the {"error": ...} 200 not-found shapes are preserved, as are the
accounting side-calls (fail-soft).

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime
from typing import Any, Dict, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "debentures_service" not in _sys.modules or not hasattr(_sys.modules.get("debentures_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("debentures_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["debentures_service"] = _pkg
    _sys.modules["debentures_service"].__path__ = [_HERE]

import httpx
import structlog
from debentures_service import crud
from debentures_service.database import Neo4jConnector
from debentures_service.dependencies import book_id_var, get_db_session, get_user_id
from debentures_service.exceptions import DebenturesServiceError
from debentures_service.models import DebentureClass, DebentureIssue, InterestPayment, RedemptionEntry
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

SERVICE_NAME = "debentures-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8058"))
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

app = FastAPI(title="Vimbai Debentures Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(DebenturesServiceError)
async def _deb_error(request: Request, exc: DebenturesServiceError):
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
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Debenture classes, issues, interest and redemptions",
    }


@app.post("/classes/create")
async def create_debenture_class(
    name: str,
    company_id: str,
    nominal_value: float,
    issue_price: float,
    coupon_rate: float,
    interest_payment_frequency: str,
    maturity_date: datetime,
    redemption_price: float,
    convertibility: str = "none",
    conversion_terms: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a debenture class."""
    deb_class = DebentureClass(
        name=name,
        company_id=company_id,
        nominal_value=nominal_value,
        issue_price=issue_price,
        coupon_rate=coupon_rate,
        interest_payment_frequency=interest_payment_frequency,
        maturity_date=maturity_date,
        redemption_price=redemption_price,
        convertibility=convertibility,
        conversion_terms=conversion_terms,
    )
    created = await crud.create_debenture_class(db_session, user_id, deb_class)
    return created


@app.post("/classes/{debenture_class_id}/issue")
async def issue_debentures(
    debenture_class_id: str,
    company_id: str,
    debentures_issued: int,
    issue_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Issue debentures."""
    deb_class = await crud.get_debenture_class(db_session, user_id, debenture_class_id)
    if not deb_class:
        return {"error": "Debenture class not found"}

    issue = DebentureIssue(
        company_id=company_id,
        debenture_class_id=debenture_class_id,
        debentures_issued=debentures_issued,
        issue_date=issue_date,
    )
    issue.total_proceeds = debentures_issued * deb_class.issue_price
    issue.discount_on_issue = debentures_issued * (deb_class.nominal_value - deb_class.issue_price)

    deb_class.debentures_issued += debentures_issued
    deb_class.debentures_outstanding += debentures_issued
    await crud.update_class_counters(
        db_session, user_id, debenture_class_id, deb_class.debentures_issued, deb_class.debentures_outstanding
    )

    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {debentures_issued} {deb_class.name} debentures",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": issue.total_proceeds, "credit": 0},
            {
                "account_code": "2330",
                "description": "Debenture Discount",
                "debit": issue.discount_on_issue,
                "credit": 0,
            },
            {
                "account_code": "2320",
                "description": "Debenture Stock",
                "debit": 0,
                "credit": debentures_issued * deb_class.nominal_value,
            },
        ],
        "reference": f"DEB-ISS-{issue.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    issue.journal_entry_id = result.get("id")
    created = await crud.create_issue(db_session, user_id, issue)
    if result.get("id"):
        await crud.update_journal_entry(
            db_session, user_id, "DebentureIssue", "OWNS_ISSUE", created.id, issue.journal_entry_id
        )
    return created


@app.post("/classes/{debenture_class_id}/interest/accrue")
async def accrue_interest(
    debenture_class_id: str,
    company_id: str,
    period_start: datetime,
    period_end: datetime,
    debentures_outstanding: int,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Accrue debenture interest."""
    deb_class = await crud.get_debenture_class(db_session, user_id, debenture_class_id)
    if not deb_class:
        return {"error": "Debenture class not found"}

    interest = InterestPayment(
        company_id=company_id,
        debenture_class_id=debenture_class_id,
        period_start=period_start,
        period_end=period_end,
        debentures_outstanding=debentures_outstanding,
        interest_rate=deb_class.coupon_rate,
    )

    # Calculate interest based on frequency
    if deb_class.interest_payment_frequency == "annual":
        periods = 1
    elif deb_class.interest_payment_frequency == "semi_annual":
        periods = 2
    elif deb_class.interest_payment_frequency == "quarterly":
        periods = 4
    else:  # monthly
        periods = 12

    annual_interest = debentures_outstanding * deb_class.nominal_value * (deb_class.coupon_rate / 100)
    interest.interest_amount = annual_interest / periods
    interest.tax_deducted = interest.interest_amount * 0.2  # Assuming 20% tax
    interest.net_payment = interest.interest_amount - interest.tax_deducted

    journal_entry = {
        "date": period_end,
        "description": f"Accrual of {deb_class.name} debenture interest",
        "entries": [
            {"account_code": "4100", "description": "Interest Expense", "debit": interest.interest_amount, "credit": 0},
            {"account_code": "2335", "description": "Interest Payable", "debit": 0, "credit": interest.interest_amount},
        ],
        "reference": f"DEB-INT-{interest.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    interest.journal_entry_id = result.get("id")
    created = await crud.create_interest(db_session, user_id, interest)
    return created


@app.post("/interest/{interest_id}/pay")
async def pay_interest(
    interest_id: str,
    payment_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Pay debenture interest."""
    interest = await crud.get_interest(db_session, user_id, interest_id)
    if not interest:
        return {"error": "Interest not found"}

    journal_entry = {
        "date": payment_date,
        "description": "Payment of debenture interest",
        "entries": [
            {"account_code": "2335", "description": "Interest Payable", "debit": interest.interest_amount, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": interest.net_payment},
            {"account_code": "2200", "description": "Tax Payable", "debit": 0, "credit": interest.tax_deducted},
        ],
        "reference": f"DEB-INT-PAY-{interest_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    interest.payment_date = payment_date
    interest.status = "paid"
    await crud.update_interest_payment(
        db_session, user_id, interest_id, payment_date, "paid", interest.journal_entry_id
    )

    return interest


@app.post("/classes/{debenture_class_id}/redeem")
async def redeem_debentures(
    debenture_class_id: str,
    company_id: str,
    debentures_redeemed: int,
    redemption_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Redeem debentures."""
    deb_class = await crud.get_debenture_class(db_session, user_id, debenture_class_id)
    if not deb_class:
        return {"error": "Debenture class not found"}

    redemption = RedemptionEntry(
        company_id=company_id,
        debenture_class_id=debenture_class_id,
        debentures_redeemed=debentures_redeemed,
        redemption_date=redemption_date,
        redemption_price=deb_class.redemption_price,
    )
    redemption.total_proceeds = debentures_redeemed * deb_class.redemption_price
    redemption.premium_on_redemption = debentures_redeemed * (deb_class.redemption_price - deb_class.nominal_value)

    deb_class.debentures_outstanding -= debentures_redeemed
    await crud.update_class_counters(
        db_session, user_id, debenture_class_id, deb_class.debentures_issued, deb_class.debentures_outstanding
    )

    journal_entry = {
        "date": redemption_date,
        "description": f"Redemption of {debentures_redeemed} {deb_class.name} debentures",
        "entries": [
            {
                "account_code": "2320",
                "description": "Debenture Stock",
                "debit": debentures_redeemed * deb_class.nominal_value,
                "credit": 0,
            },
            {
                "account_code": "4100",
                "description": "Premium on Redemption",
                "debit": redemption.premium_on_redemption,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": redemption.total_proceeds},
        ],
        "reference": f"DEB-RED-{redemption.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    redemption.journal_entry_id = result.get("id")
    created = await crud.create_redemption(db_session, user_id, redemption)
    return created


@app.get("/classes")
async def list_debenture_classes(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's debenture classes."""
    result = await crud.list_debenture_classes(db_session, user_id)
    if company_id:
        result = [d for d in result if d.company_id == company_id]
    return {"debenture_classes": result}


@app.get("/issues")
async def list_issues(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's debenture issues."""
    result = await crud.list_issues(db_session, user_id)
    if company_id:
        result = [i for i in result if i.company_id == company_id]
    return {"issues": result}


@app.get("/interest")
async def list_interest_payments(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's interest payments."""
    result = await crud.list_interest(db_session, user_id)
    if company_id:
        result = [i for i in result if i.company_id == company_id]
    return {"interest_payments": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
