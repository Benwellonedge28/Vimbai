"""Vimbai Sovereign Treasury Service - national accounts, debt and fiscal positions. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "sovereign_treasury_service" not in _sys.modules or not hasattr(
    _sys.modules.get("sovereign_treasury_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("sovereign_treasury_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["sovereign_treasury_service"] = _pkg
    _sys.modules["sovereign_treasury_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from sovereign_treasury_service import crud, models
from sovereign_treasury_service.dependencies import book_id_var, get_db_session, get_user_id
from sovereign_treasury_service.exceptions import SovereignTreasuryError

SERVICE_NAME = "sovereign-treasury-service"
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
app = FastAPI(title="Vimbai Sovereign Treasury Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(SovereignTreasuryError)
async def _sovereign_treasury_error(request: Request, exc: SovereignTreasuryError):
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
async def create_account(
    account: models.SovereignAccount,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_account(db_session, user_id, account)
    logger.info("account_created", country=item.country, type=item.account_type, balance=item.balance)
    return {"id": item.id, "type": item.account_type, "balance": item.balance}


@app.get("/accounts/{country}")
async def get_accounts(
    country: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    accounts = await crud.list_accounts(db_session, user_id, country)
    return {"country": country, "accounts": accounts, "total_balance": sum(a.balance for a in accounts)}


@app.post("/debt")
async def register_debt(
    debt: models.SovereignDebt,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.register_debt(db_session, user_id, debt)
    logger.info("debt_registered", country=item.country, instrument=item.instrument, outstanding=item.outstanding)
    return {"id": item.id, "instrument": item.instrument, "outstanding": item.outstanding}


@app.get("/debt/{country}")
async def get_debt(
    country: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    debts = await crud.list_debts(db_session, user_id, country)
    total = sum(d.outstanding for d in debts)
    return {"country": country, "debts": debts, "total_debt": total, "instruments": len(debts)}


@app.post("/fiscal-position", response_model=models.FiscalPosition)
async def set_fiscal_position(
    pos: models.FiscalPosition,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.set_fiscal_position(db_session, user_id, pos)
    logger.info("fiscal_position_set", country=item.country, year=item.fiscal_year)
    return item


@app.get("/fiscal-position/{country}", response_model=models.FiscalPosition)
async def get_fiscal_position(
    country: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.get_fiscal_position(db_session, user_id, country)
    except SovereignTreasuryError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
