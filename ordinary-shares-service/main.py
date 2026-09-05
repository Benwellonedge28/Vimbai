# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "ordinary_shares_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Ordinary Shares Service
Manages ordinary share dividends.

Record-keeping only: this service records dividend declarations and
payment journal-entry references (user-owned, Book-scoped via X-User-Id /
X-Book-ID); it never moves money. The /pay endpoint records that a
dividend was paid. Corrections use reversing entries.
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
from ordinary_shares_service import crud, models
from ordinary_shares_service.database import Neo4jConnector
from ordinary_shares_service.dependencies import book_id_var, get_db_session, get_user_id
from ordinary_shares_service.exceptions import ConflictError, NotFoundError, ValidationError

SERVICE_NAME = "ordinary-shares-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8049"))
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

app = FastAPI(title="Vimbai Ordinary Shares Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Ordinary shares management"}


@app.post("/dividends/declare")
async def declare_dividend(
    company_id: str,
    dividend_type: str,
    per_share_amount: float,
    total_shares: int,
    record_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Declare ordinary dividend."""
    dividend = models.OrdinaryDividend(
        company_id=company_id,
        dividend_type=dividend_type,
        per_share_amount=per_share_amount,
        total_shares=total_shares,
        record_date=record_date,
    )
    dividend.total_dividend = per_share_amount * total_shares
    journal_entry = {
        "date": record_date,
        "description": f"Declaration of {dividend_type} dividend",
        "entries": [
            {
                "account_code": "3300",
                "description": "Retained Earnings",
                "debit": dividend.total_dividend,
                "credit": 0,
            },
            {"account_code": "2310", "description": "Dividend Payable", "debit": 0, "credit": dividend.total_dividend},
        ],
        "reference": f"DIV-{dividend.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.journal_entry_id = result.get("id")
    return await crud.create_dividend(db_session, user_id, dividend)


@app.post("/dividends/{dividend_id}/pay")
async def pay_dividend(
    dividend_id: str,
    payment_date: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record dividend payment (records the payment + journal entry; moves no money)."""
    dividend = await crud.get_dividend(db_session, user_id, dividend_id)
    if not dividend:
        return {"error": "Dividend not found"}
    journal_entry = {
        "date": payment_date,
        "description": f"Payment of {dividend.dividend_type} dividend",
        "entries": [
            {
                "account_code": "2310",
                "description": "Dividend Payable",
                "debit": dividend.total_dividend,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": dividend.total_dividend},
        ],
        "reference": f"DIV-PAY-{dividend_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.payment_date = payment_date
    dividend.status = "paid"
    await crud.save_dividend(db_session, user_id, dividend)
    return dividend


@app.get("/dividends")
async def list_dividends(
    company_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List dividends."""
    result = await crud.list_dividends(db_session, user_id)
    if company_id:
        result = [d for d in result if d.company_id == company_id]
    return {"dividends": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
