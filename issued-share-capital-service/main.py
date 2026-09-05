# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "issued_share_capital_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Issued Share Capital Service
Manages issued shares and allotments.

Record-keeping only: this service records share capital movements and
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
from issued_share_capital_service import crud, models
from issued_share_capital_service.database import Neo4jConnector
from issued_share_capital_service.dependencies import book_id_var, get_db_session, get_user_id
from issued_share_capital_service.exceptions import ConflictError, NotFoundError, ValidationError
from neo4j import AsyncSession

SERVICE_NAME = "issued-share-capital-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8048"))
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

app = FastAPI(title="Vimbai Issued Share Capital Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Issued share capital management"}


@app.post("/issue")
async def issue_shares(
    company_id: str,
    issue_date: datetime,
    share_class: str,
    shares_issued: int,
    issue_price: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Issue new shares."""
    issue = models.ShareIssue(
        company_id=company_id,
        issue_date=issue_date,
        share_class=share_class,
        shares_issued=shares_issued,
        issue_price=issue_price,
    )
    issue.total_proceeds = shares_issued * issue_price
    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {shares_issued} {share_class} shares at {issue_price}",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": issue.total_proceeds, "credit": 0},
            {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": shares_issued * 1},
            {
                "account_code": "3210",
                "description": "Share Premium",
                "debit": 0,
                "credit": issue.total_proceeds - shares_issued,
            },
        ],
        "reference": f"ISS-{issue.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    issue.journal_entry_id = result.get("id")
    return await crud.create_share_issue(db_session, user_id, issue)


@app.post("/shareholders/register")
async def register_shareholder(
    company_id: str,
    name: str,
    address: str,
    shares_held: int = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a shareholder."""
    holder = models.Shareholder(company_id=company_id, name=name, address=address, shares_held=shares_held)
    return await crud.create_shareholder(db_session, user_id, holder)


@app.get("/shareholders/{company_id}")
async def get_shareholders(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get company shareholders."""
    all_holders = await crud.list_shareholders(db_session, user_id)
    return {"shareholders": [h for h in all_holders if h.company_id == company_id]}


@app.get("/issues/{company_id}")
async def get_share_issues(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get all share issues for a company."""
    all_issues = await crud.list_share_issues(db_session, user_id)
    return {"issues": [i for i in all_issues if i.company_id == company_id]}


@app.get("/summary/{company_id}")
async def get_capital_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get share capital summary."""
    all_issues = await crud.list_share_issues(db_session, user_id)
    company_issues = [i for i in all_issues if i.company_id == company_id]
    total_shares = sum(i.shares_issued for i in company_issues)
    total_capital = sum(i.total_proceeds for i in company_issues)
    return {"total_shares_issued": total_shares, "total_proceeds": total_capital}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
