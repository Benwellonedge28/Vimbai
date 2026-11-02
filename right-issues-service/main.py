"""
Vimbai Right Issues Service
Rights offers, acceptances and renunciations.

Right issues and renunciations live in Neo4j, scoped to the caller and the Book.
The file may be imported bare (bracket mounts, uvicorn main:app), so it bootstraps
its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "right_issues_service" not in _sys.modules or not hasattr(_sys.modules.get("right_issues_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("right_issues_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["right_issues_service"] = _pkg
    _sys.modules["right_issues_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from right_issues_service import crud
from right_issues_service.dependencies import book_id_var, get_db_session, get_user_id
from right_issues_service.exceptions import NotFoundError

SERVICE_NAME = "right-issues-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8051"))
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

app = FastAPI(title="Vimbai Right Issues Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Right issues management"}


@app.post("/offer")
async def offer_rights(
    company_id: str,
    issue_date: datetime,
    expiry_date: datetime,
    share_class: str,
    rights_offered: int,
    new_shares_offered: int,
    issue_price: float,
    nominal_value: float,
    entitlements: Dict[str, int],
    renouncement_allowed: bool = True,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Offer rights issue to existing shareholders."""
    return await crud.offer_rights(
        db_session,
        user_id,
        company_id=company_id,
        issue_date=issue_date,
        expiry_date=expiry_date,
        share_class=share_class,
        rights_offered=rights_offered,
        new_shares_offered=new_shares_offered,
        issue_price=issue_price,
        nominal_value=nominal_value,
        entitlements=entitlements,
        renouncement_allowed=renouncement_allowed,
    )


@app.post("/{right_issue_id}/accept")
async def accept_rights(
    right_issue_id: str,
    shareholder_id: str,
    shares_accepted: int,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record acceptance of rights issue."""
    try:
        right_issue = await crud.record_acceptance(db_session, user_id, right_issue_id, shareholder_id, shares_accepted)
    except NotFoundError:
        return {"error": "Right issue not found"}

    acceptance_value = shares_accepted * right_issue.issue_price
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Acceptance of rights issue - {shares_accepted} shares",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": acceptance_value, "credit": 0},
            {
                "account_code": "3200",
                "description": "Share Capital",
                "debit": 0,
                "credit": shares_accepted * right_issue.nominal_value,
            },
            {
                "account_code": "3210",
                "description": "Share Premium",
                "debit": 0,
                "credit": acceptance_value - (shares_accepted * right_issue.nominal_value),
            },
        ],
        "reference": f"RIGHT-ACCEPT-{right_issue_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    return right_issue


@app.post("/{right_issue_id}/renounce")
async def renounce_rights(
    right_issue_id: str,
    shareholder_id: str,
    new_shareholder_id: str,
    renunciation_price: float,
    renunciation_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record renunciation of rights to another party."""
    try:
        right_issue = await crud.get_right_issue(db_session, user_id, right_issue_id)
    except NotFoundError:
        return {"error": "Right issue not found"}

    if not right_issue.renouncement_allowed:
        return {"error": "Renouncement not allowed for this issue"}

    right_issue, renunciation = await crud.record_renunciation(
        db_session,
        user_id,
        right_issue_id,
        shareholder_id,
        new_shareholder_id,
        renunciation_price,
        renunciation_date,
    )

    # Journal entry for renunciation consideration
    journal_entry = {
        "date": renunciation_date,
        "description": "Rights renunciation payment",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": renunciation_price, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": renunciation_price},
        ],
        "reference": f"RIGHT-REN-{right_issue_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    return {"right_issue": right_issue, "renunciation": renunciation}


@app.get("/right-issues")
async def list_right_issues(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all right issues."""
    issues = await crud.list_right_issues(db_session, user_id, company_id)
    return {"right_issues": issues}


@app.get("/{right_issue_id}")
async def get_right_issue(
    right_issue_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get right issue details."""
    try:
        return await crud.get_right_issue(db_session, user_id, right_issue_id)
    except NotFoundError:
        return {"error": "Right issue not found"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
