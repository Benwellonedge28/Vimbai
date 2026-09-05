# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "bonus_shares_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Bonus Shares Service
Issues bonus shares from reserves.

Record-keeping only: this service records bonus share issuances and
journal-entry references (user-owned, Book-scoped via X-User-Id /
X-Book-ID); it never moves money. Corrections use reversing entries.
"""

import os
from datetime import datetime
from typing import Dict, Optional

import httpx
import structlog
from bonus_shares_service import crud, models
from bonus_shares_service.database import Neo4jConnector
from bonus_shares_service.dependencies import book_id_var, get_db_session, get_user_id
from bonus_shares_service.exceptions import ConflictError, NotFoundError, ValidationError
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

SERVICE_NAME = "bonus-shares-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8050"))
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

app = FastAPI(title="Vimbai Bonus Shares Service", version=SERVICE_VERSION, docs_url="/docs")
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


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict:
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Bonus shares issuance"}


@app.post("/issue")
async def issue_bonus_shares(
    company_id: str,
    issue_date: datetime,
    shares_issued: int,
    nominal_value: float,
    source_reserve: str,
    shareholder_allocations: Dict[str, int],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Issue bonus shares from reserves."""
    issue = models.BonusIssue(
        company_id=company_id,
        issue_date=issue_date,
        shares_issued=shares_issued,
        nominal_value=nominal_value,
        source_reserve=source_reserve,
        shareholder_allocations=shareholder_allocations,
    )
    issue.total_nominal_value = shares_issued * nominal_value
    issue.amount_utilized = issue.total_nominal_value
    reserve_account = {"share_premium": "3210", "retained_earnings": "3300", "general_reserve": "3310"}.get(
        source_reserve, "3300"
    )
    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {shares_issued} bonus shares from {source_reserve}",
        "entries": [
            {
                "account_code": reserve_account,
                "description": f"{source_reserve} Reserve",
                "debit": issue.amount_utilized,
                "credit": 0,
            },
            {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": issue.total_nominal_value},
        ],
        "reference": f"BONUS-{issue.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    issue.journal_entry_id = result.get("id")
    return await crud.create_bonus_issue(db_session, user_id, issue)


@app.get("/issues")
async def list_bonus_issues(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List bonus issues."""
    result = await crud.list_bonus_issues(db_session, user_id)
    if company_id:
        result = [i for i in result if i.company_id == company_id]
    return {"issues": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
