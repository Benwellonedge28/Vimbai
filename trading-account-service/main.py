"""
Vimbai Trading Account Service
Calculates trading account: gross profit, cost of sales, and net trading margins.

Generated accounts live in Neo4j, scoped to the caller and the Book. The file may
be imported bare (bracket mounts, uvicorn main:app), so it bootstraps its own
package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "trading_account_service" not in _sys.modules or not hasattr(
    _sys.modules.get("trading_account_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("trading_account_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["trading_account_service"] = _pkg
    _sys.modules["trading_account_service"].__path__ = [_HERE]

import os
from typing import List, Optional

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from trading_account_service import crud
from trading_account_service.dependencies import book_id_var, get_db_session, get_user_id
from trading_account_service.exceptions import NotFoundError
from trading_account_service.models import TradingAccountRequest, TradingAccountResult

SERVICE_NAME = "trading-account-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8000"))

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

app = FastAPI(title="Vimbai Trading Account Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/generate", response_model=TradingAccountResult)
async def generate_trading_account(
    request: TradingAccountRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate a trading account from the given inputs."""
    result = await crud.generate(db_session, user_id, request)
    logger.info(
        "Trading account generated",
        account_id=result.id,
        period=request.period,
        gp=result.gross_profit,
    )
    return result


@app.get("/accounts", response_model=List[TradingAccountResult])
async def list_accounts(
    period: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List trading accounts."""
    return await crud.list_accounts(db_session, user_id, period)


@app.get("/accounts/{account_id}", response_model=TradingAccountResult)
async def get_account(
    account_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific trading account."""
    try:
        return await crud.get_account(db_session, user_id, account_id)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Trading account not found")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
