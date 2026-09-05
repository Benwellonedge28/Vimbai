# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "share_redemption_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Share Redemption Service
Manages share redemptions, funding fresh issues, and CRR requirements.

Record-keeping only: this service records share redemption movements and
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
from share_redemption_service import crud, models
from share_redemption_service.database import Neo4jConnector
from share_redemption_service.dependencies import book_id_var, get_db_session, get_user_id
from share_redemption_service.exceptions import ConflictError, NotFoundError, ValidationError

SERVICE_NAME = "share-redemption-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8053"))
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

app = FastAPI(title="Vimbai Share Redemption Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Share redemption management"}


@app.post("/redemptions/initiate")
async def initiate_redemption(
    company_id: str,
    share_class: str,
    shares_redeemed: int,
    nominal_value: float,
    redemption_price: float,
    redemption_date: datetime,
    redemption_method: str,
    authority_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Initiate share redemption."""
    redemption = models.ShareRedemption(
        company_id=company_id,
        share_class=share_class,
        shares_redeemed=shares_redeemed,
        nominal_value=nominal_value,
        redemption_price=redemption_price,
        redemption_date=redemption_date,
        redemption_method=redemption_method,
        authority_date=authority_date,
    )
    redemption.total_redemption_value = shares_redeemed * redemption_price
    # For redemption out of proceeds, CRR must equal the nominal value of shares redeemed
    # less the proceeds of a fresh issue
    if redemption_method == "proceeds":
        crr_req = models.CRRRequirement(
            redemption_id=redemption.id,
            nominal_value_of_shares=shares_redeemed * nominal_value,
            proceeds_used=redemption.total_redemption_value,
            fresh_issue_proceeds=0,
            minimum_crr_required=shares_redeemed * nominal_value,
            crr_created=0,
            source_of_crr="requires_calculation",
        )
        await crud.create_crr_requirement(db_session, user_id, crr_req)
        redemption.status = "awaiting_crr"
    return await crud.create_redemption(db_session, user_id, redemption)


@app.post("/redemptions/{redemption_id}/fresh-issue")
async def record_fresh_issue(
    redemption_id: str,
    shares_issued: int,
    issue_price: float,
    nominal_value: float,
    issue_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record fresh issue to fund redemption."""
    redemption = await crud.get_redemption(db_session, user_id, redemption_id)
    if not redemption:
        return {"error": "Redemption not found"}
    fresh_issue = models.FreshIssueForRedemption(
        redemption_id=redemption_id,
        shares_issued=shares_issued,
        issue_price=issue_price,
        nominal_value=nominal_value,
        issue_date=issue_date,
    )
    fresh_issue.total_proceeds = shares_issued * issue_price
    # Record fresh issue journal entry
    journal_entry = {
        "date": issue_date,
        "description": "Fresh issue to fund redemption",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": fresh_issue.total_proceeds, "credit": 0},
            {
                "account_code": "3200",
                "description": "Share Capital",
                "debit": 0,
                "credit": shares_issued * nominal_value,
            },
            {
                "account_code": "3210",
                "description": "Share Premium",
                "debit": 0,
                "credit": fresh_issue.total_proceeds - (shares_issued * nominal_value),
            },
        ],
        "reference": f"FRESH-ISS-{fresh_issue.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    fresh_issue.journal_entry_id = result.get("id")
    return await crud.create_fresh_issue(db_session, user_id, fresh_issue)


@app.post("/redemptions/{redemption_id}/complete")
async def complete_redemption(
    redemption_id: str,
    statutory_declaration_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Complete share redemption after statutory requirements."""
    redemption = await crud.get_redemption(db_session, user_id, redemption_id)
    if not redemption:
        return {"error": "Redemption not found"}
    if statutory_declaration_date is None:
        statutory_declaration_date = datetime.now(timezone.utc)
    redemption.statutory_declaration_date = statutory_declaration_date
    # Determine CRR amount based on method
    if redemption.redemption_method == "fresh_issue":
        # No CRR required if fully funded by fresh issue
        crr_amount = 0
    else:
        # CRR = nominal value of shares redeemed (proceeds / existing_assets / combination)
        crr_amount = redemption.shares_redeemed * redemption.nominal_value
    # Create journal entry for redemption
    capital_account = "3205" if redemption.share_class == "preference" else "3200"
    entries = [
        {
            "account_code": capital_account,
            "description": "Share Capital",
            "debit": redemption.shares_redeemed * redemption.nominal_value,
            "credit": 0,
        },
    ]
    if crr_amount > 0:
        entries.append(
            {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": crr_amount, "credit": 0}
        )
    entries.append(
        {"account_code": "1000", "description": "Bank", "debit": 0, "credit": redemption.total_redemption_value}
    )
    journal_entry = {
        "date": redemption.redemption_date,
        "description": f"Redemption of {redemption.shares_redeemed} {redemption.share_class} shares",
        "entries": entries,
        "reference": f"SH-RED-{redemption.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    redemption.journal_entry_id = result.get("id")
    redemption.status = "completed"
    await crud.save_redemption(db_session, user_id, redemption)
    return {"redemption": redemption, "crr_created": crr_amount}


@app.get("/redemptions")
async def list_redemptions(
    company_id: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List share redemptions."""
    result = await crud.list_redemptions(db_session, user_id)
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    if status:
        result = [r for r in result if r.status == status]
    return {"redemptions": result}


@app.get("/redemptions/{redemption_id}")
async def get_redemption(
    redemption_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get redemption details."""
    redemption = await crud.get_redemption(db_session, user_id, redemption_id)
    if not redemption:
        return {"error": "Redemption not found"}
    all_fresh_issues = await crud.list_fresh_issues(db_session, user_id)
    all_crr = await crud.list_crr_requirements(db_session, user_id)
    redemption_fresh_issues = [f for f in all_fresh_issues if f.redemption_id == redemption_id]
    redemption_crr = next((c for c in all_crr if c.redemption_id == redemption_id), None)
    return {"redemption": redemption, "fresh_issues": redemption_fresh_issues, "crr_requirement": redemption_crr}


@app.get("/crr-requirements")
async def list_crr_requirements(
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List CRR requirements."""
    result = await crud.list_crr_requirements(db_session, user_id)
    if status:
        result = [c for c in result if c.compliance_status == status]
    return {"crr_requirements": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
