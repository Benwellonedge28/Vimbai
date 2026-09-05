"""Vimbai Fund Accounting Service - Track funds and fund transactions. Port: 8345

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "fund_accounting_service" not in _sys.modules or not hasattr(
    _sys.modules.get("fund_accounting_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("fund_accounting_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["fund_accounting_service"] = _pkg
    _sys.modules["fund_accounting_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fund_accounting_service import crud, models
from fund_accounting_service.dependencies import book_id_var, get_db_session, get_user_id
from neo4j import AsyncSession

SERVICE_NAME = "fund-accounting-service"
PORT = int(os.getenv("PORT", "8345"))
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
app = FastAPI(title="Vimbai Fund Accounting Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="fund-accounting-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/funds", response_model=models.Fund)
async def create_fund(
    fund: models.FundCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_fund(db_session, user_id, fund)
    logger.info(
        "fund_created",
        company_id=item.company_id,
        fund_name=item.fund_name,
        net_assets=item.net_assets,
    )
    return item


@app.get("/funds/{company_id}")
async def get_funds(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    funds = await crud.get_funds(db_session, user_id, company_id)
    return {
        "company_id": company_id,
        "funds": funds,
        "total_balance": sum(f.balance for f in funds),
        "total_net_assets": sum(f.net_assets for f in funds),
    }


@app.post("/transactions")
async def record_transaction(
    tx: models.FundTransactionCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.record_transaction(db_session, user_id, tx)
    if item is None:
        raise HTTPException(
            status_code=404,
            detail="Fund not found or not visible in this Book",
        )
    logger.info(
        "fund_transaction_recorded",
        fund_id=item.fund_id,
        amount=item.amount,
        is_income=item.is_income,
    )
    return {"id": item.id, "status": "recorded"}


@app.get("/transactions/{fund_id}")
async def get_transactions(
    fund_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    txs = await crud.get_transactions(db_session, user_id, fund_id)
    return {"fund_id": fund_id, "transactions": txs, "total": len(txs)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
