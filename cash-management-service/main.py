"""
Vimbai Cash Management Service
Manages cash positions, transfers, and short-term liquidity.

Accounts, transfers, and liquidity positions persist in Neo4j, stamped with
the caller (X-User-Id) and the Book context (X-Book-ID verified upstream by
the API gateway). Transfers only move balances between the caller's own
Book-visible accounts; liquidity is computed over the caller's accounts
only. The original 200/400/404 status codes are preserved.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "cash_management_service" not in _sys.modules or not hasattr(
    _sys.modules.get("cash_management_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("cash_management_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["cash_management_service"] = _pkg
    _sys.modules["cash_management_service"].__path__ = [_HERE]

import structlog
from cash_management_service import crud
from cash_management_service.database import Neo4jConnector
from cash_management_service.dependencies import book_id_var, get_db_session, get_user_id
from cash_management_service.exceptions import CashManagementError
from cash_management_service.models import CashAccount, CashTransfer, LiquidityPosition
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "cash-management-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8264"))

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

app = FastAPI(title="Vimbai Cash Management Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(CashManagementError)
async def _cash_management_error(request: Request, exc: CashManagementError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/accounts", response_model=CashAccount)
async def create_account(
    account_name: str,
    bank: str,
    account_number: str,
    currency: str = "USD",
    balance: float = 0.0,
    min_balance: float = 0.0,
    type: str = "operating",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register a cash account."""
    valid_types = ["operating", "reserve", "investment"]
    if type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid account type. Must be one of {valid_types}")

    account = CashAccount(
        account_name=account_name,
        bank=bank,
        account_number=account_number,
        currency=currency,
        balance=balance,
        min_balance=min_balance,
        type=type,
    )
    created = await crud.create_account(db_session, user_id, account)
    logger.info("Cash account created", account_id=created.id, name=account_name)
    return created


@app.get("/accounts", response_model=List[CashAccount])
async def list_accounts(
    type: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's cash accounts."""
    return await crud.list_accounts(db_session, user_id, type=type)


@app.post("/transfers", response_model=CashTransfer)
async def create_transfer(
    from_account_id: str,
    to_account_id: str,
    amount: float,
    currency: str = "USD",
    reference: str = "",
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a cash transfer between the caller's accounts."""
    accounts = await crud.list_accounts(db_session, user_id)
    from_acct = next((a for a in accounts if a.id == from_account_id), None)
    to_acct = next((a for a in accounts if a.id == to_account_id), None)
    if not from_acct or not to_acct:
        raise HTTPException(status_code=404, detail="Source or destination account not found")
    if from_acct.balance - amount < from_acct.min_balance:
        raise HTTPException(status_code=400, detail="Transfer would breach minimum balance")

    transfer = CashTransfer(
        from_account_id=from_account_id,
        to_account_id=to_account_id,
        amount=amount,
        currency=currency,
        reference=reference,
        notes=notes,
        status="completed",
    )
    # Balances move on the caller's own accounts only (Python-computed SET).
    from_acct.balance -= amount
    to_acct.balance += amount
    await crud.set_balance(db_session, user_id, from_acct.id, from_acct.balance)
    await crud.set_balance(db_session, user_id, to_acct.id, to_acct.balance)
    created = await crud.create_transfer(db_session, user_id, transfer)
    logger.info("Cash transfer completed", transfer_id=created.id, amount=amount)
    return created


@app.get("/transfers", response_model=List[CashTransfer])
async def list_transfers(
    limit: int = 50,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's cash transfers."""
    return await crud.list_transfers(db_session, user_id, limit=limit)


@app.post("/liquidity", response_model=LiquidityPosition)
async def calculate_liquidity(
    short_term_obligations: float = 0.0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate the caller's current liquidity position."""
    accounts = await crud.list_accounts(db_session, user_id)
    operating = sum(a.balance for a in accounts if a.type == "operating")
    reserve = sum(a.balance for a in accounts if a.type == "reserve")
    invested = sum(a.balance for a in accounts if a.type == "investment")
    total = operating + reserve + invested
    ratio = (total / short_term_obligations) if short_term_obligations > 0 else 0.0

    position = LiquidityPosition(
        position_date=datetime.now(timezone.utc),
        total_cash=total,
        operating_cash=operating,
        reserve_cash=reserve,
        invested_cash=invested,
        short_term_obligations=short_term_obligations,
        liquidity_ratio=round(ratio, 2),
    )
    saved = await crud.save_position(db_session, user_id, position)
    logger.info("Liquidity position calculated", total=total, ratio=ratio)
    return saved


@app.get("/liquidity", response_model=List[LiquidityPosition])
async def list_liquidity_positions(
    limit: int = 30,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's historical liquidity positions."""
    return await crud.list_positions(db_session, user_id, limit=limit)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
