"""Vimbai Cash Optimization Service - idle cash redeployment suggestions. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "cash_optimization_service" not in _sys.modules or not hasattr(
    _sys.modules.get("cash_optimization_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("cash_optimization_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["cash_optimization_service"] = _pkg
    _sys.modules["cash_optimization_service"].__path__ = [_HERE]

import os

import structlog
from cash_optimization_service import crud, models
from cash_optimization_service.dependencies import book_id_var, get_db_session, get_user_id
from cash_optimization_service.exceptions import CashOptimizationError
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "cash-optimization-service"
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
app = FastAPI(title="Vimbai Cash Optimization Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(CashOptimizationError)
async def _cash_optimization_error(request: Request, exc: CashOptimizationError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/accounts")
async def add_account(
    account: models.CashAccountCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.add_account(db_session, user_id, account)
    logger.info("cash_account_added", company_id=account.company_id, name=account.account_name)
    return item


@app.get("/accounts/{company_id}")
async def get_accounts(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_accounts(db_session, user_id, company_id)


@app.post("/optimize/{company_id}")
async def run_optimization(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    result = await crud.run_optimization(db_session, user_id, company_id)
    logger.info(
        "optimization_run",
        company_id=company_id,
        suggestions=result["total_count"],
        benefit=result["potential_annual_benefit"],
    )
    return result


@app.get("/suggestions/{company_id}")
async def get_suggestions(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_suggestions(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
