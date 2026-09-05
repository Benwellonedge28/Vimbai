"""Vimbai Debt Management Service - Loan tracking, amortization schedules, debt restructuring, and covenant monitoring. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "debt_management_service" not in _sys.modules or not hasattr(
    _sys.modules.get("debt_management_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("debt_management_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["debt_management_service"] = _pkg
    _sys.modules["debt_management_service"].__path__ = [_HERE]

import os
from typing import List

import structlog
from debt_management_service import crud, models
from debt_management_service.dependencies import book_id_var, get_db_session, get_user_id
from debt_management_service.exceptions import DebtManagementError, NotFoundError
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "debt-management-service"
PORT = int(os.getenv("PORT", "8370"))
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
app = FastAPI(title="Vimbai Debt Management Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(NotFoundError)
@app.exception_handler(DebtManagementError)
async def _debt_management_error(request: Request, exc: DebtManagementError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/loans", response_model=models.Loan)
async def create_loan(
    loan: models.LoanCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_loan(db_session, user_id, loan)
    logger.info(
        "loan_created",
        company_id=item.company_id,
        loan_name=item.loan_name,
        principal=item.principal,
    )
    return item


@app.get("/loans", response_model=List[models.Loan])
async def list_loans(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_loans(db_session, user_id, company_id)


@app.post("/loans/{loan_id}/schedule", response_model=List[models.AmortizationScheduleItem])
async def get_amortization_schedule(
    loan_id: str,
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    loan = await crud.get_loan(db_session, user_id, company_id, loan_id)
    return crud.build_schedule(loan)


@app.get("/summary", response_model=models.DebtSummary)
async def get_debt_summary(
    company_id: str,
    equity: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_debt_summary(db_session, user_id, company_id, equity)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
