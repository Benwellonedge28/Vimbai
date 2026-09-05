"""Vimbai Equity Changes Service - Track equity changes. Port: 8344

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "equity_changes_service" not in _sys.modules or not hasattr(_sys.modules.get("equity_changes_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("equity_changes_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["equity_changes_service"] = _pkg
    _sys.modules["equity_changes_service"].__path__ = [_HERE]

import os
from typing import Optional

import structlog
from equity_changes_service import crud, models
from equity_changes_service.dependencies import book_id_var, get_db_session, get_user_id
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "equity-changes-service"
PORT = int(os.getenv("PORT", "8344"))
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
app = FastAPI(title="Vimbai Equity Changes Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="equity-changes-service", instrument_app=app)
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


@app.post("/transactions", response_model=models.EquityTransaction)
async def create_transaction(
    tx: models.EquityTransactionCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_transaction(db_session, user_id, tx)
    logger.info(
        "equity_transaction_created",
        company_id=item.company_id,
        transaction_type=item.transaction_type.value,
        amount=item.amount,
    )
    return item


@app.get("/transactions/{company_id}")
async def get_transactions(
    company_id: str,
    tx_type: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    txs = await crud.get_transactions(db_session, user_id, company_id, tx_type)
    return {"company_id": company_id, "transactions": txs, "total": len(txs)}


@app.post("/statement", response_model=models.EquityStatement)
async def generate_statement(
    stmt: models.EquityStatementCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.generate_statement(db_session, user_id, stmt)
    logger.info(
        "equity_statement_generated",
        company_id=item.company_id,
        period=item.period,
        ending_equity=item.ending_equity,
    )
    return item


@app.get("/statements/{company_id}")
async def get_statements(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    statements = await crud.get_statements(db_session, user_id, company_id)
    return {"company_id": company_id, "statements": statements, "total": len(statements)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
