"""Vimbai Expense Tracking Service - Track and categorize business expenses. Port: 8348"""

# This file may be imported bare (bracket mounts, uvicorn main:app), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "expense_tracking_service" not in _sys.modules or not hasattr(
    _sys.modules.get("expense_tracking_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("expense_tracking_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules["expense_tracking_service"] = _pkg

import os
import uuid  # noqa: F401  (kept for API-compat with previous module surface)
from collections import defaultdict  # noqa: F401
from typing import Optional

import structlog
from dotenv import load_dotenv
from expense_tracking_service import crud, models
from expense_tracking_service.database import Neo4jConnector
from expense_tracking_service.dependencies import book_id_var, get_db_session, get_user_id
from expense_tracking_service.exceptions import ExpenseError, NotFoundError
from fastapi import Depends, FastAPI, Query, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

load_dotenv()

SERVICE_NAME = "expense-tracking-service"
PORT = int(os.getenv("PORT", "8348"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Expense Tracking Service", version="3.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Bind the gateway-verified X-Book-ID into the request-scoped contextvar."""
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


@app.exception_handler(ExpenseError)
async def expense_error_handler(request: Request, exc: ExpenseError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=exc.status_code,
        content={"detail": exc.detail, "code": exc.code},
    )


@app.on_event("startup")
async def startup():
    try:
        await Neo4jConnector.verify_connectivity()
        logger.info("Neo4j connectivity verified")
    except Exception:  # pragma: no cover - dev mode without database
        logger.warning("Neo4j not reachable; running without connectivity check")


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/expenses", response_model=models.Expense)
async def create_expense(
    expense: models.ExpenseCreate,
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_expense(session, user_id, expense)


@app.get("/expenses/{company_id}")
async def get_expenses(
    company_id: str,
    category: Optional[str] = None,
    status_filter: Optional[str] = None,
    limit: int = Query(100, le=1000),
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    expenses = await crud.get_expenses(session, user_id, company_id, category, status_filter, limit)
    return {"company_id": company_id, "expenses": expenses, "total": len(expenses)}


@app.put("/expenses/{expense_id}/approve")
async def approve_expense(
    expense_id: str,
    approver: str = "",
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    expense = await crud.approve_expense(session, user_id, expense_id, approver)
    return {"id": expense.id, "status": expense.status.value, "approved_by": expense.approved_by}


@app.put("/expenses/{expense_id}/reject")
async def reject_expense(
    expense_id: str,
    reason: str = "",
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    expense = await crud.reject_expense(session, user_id, expense_id, reason)
    return {"id": expense.id, "status": expense.status.value, "rejection_reason": expense.rejection_reason}


@app.get("/summary/{company_id}")
async def expense_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    return await crud.expense_summary(session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
