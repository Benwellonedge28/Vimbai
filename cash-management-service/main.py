"""
Vimbai Cash Management Service
Manages cash positions, transfers, and short-term liquidity.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "cash-management-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8264"))

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


class CashAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    account_name: str
    bank: str
    account_number: str
    currency: str = "USD"
    balance: float = 0.0
    min_balance: float = 0.0
    type: str = "operating"  # operating, reserve, investment
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashTransfer(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    from_account_id: str
    to_account_id: str
    amount: float
    currency: str = "USD"
    transfer_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    status: str = "pending"  # pending, completed, failed
    reference: str = ""
    notes: str = ""


class LiquidityPosition(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    position_date: datetime
    total_cash: float
    operating_cash: float
    reserve_cash: float
    invested_cash: float
    short_term_obligations: float
    liquidity_ratio: float = 0.0


accounts: List[CashAccount] = []
transfers: List[CashTransfer] = []
positions: List[LiquidityPosition] = []


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
    accounts.append(account)
    logger.info("Cash account created", account_id=account.id, name=account_name)
    return account


@app.get("/accounts", response_model=List[CashAccount])
async def list_accounts(type: Optional[str] = None):
    """List cash accounts."""
    if type:
        return [a for a in accounts if a.type == type]
    return accounts


@app.post("/transfers", response_model=CashTransfer)
async def create_transfer(
    from_account_id: str, to_account_id: str, amount: float, currency: str = "USD", reference: str = "", notes: str = ""
):
    """Create a cash transfer between accounts."""
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
    from_acct.balance -= amount
    to_acct.balance += amount
    transfers.append(transfer)
    logger.info("Cash transfer completed", transfer_id=transfer.id, amount=amount)
    return transfer


@app.get("/transfers", response_model=List[CashTransfer])
async def list_transfers(limit: int = 50):
    """List cash transfers."""
    return transfers[-limit:]


@app.post("/liquidity", response_model=LiquidityPosition)
async def calculate_liquidity(short_term_obligations: float = 0.0):
    """Calculate current liquidity position."""
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
    positions.append(position)
    logger.info("Liquidity position calculated", total=total, ratio=ratio)
    return position


@app.get("/liquidity", response_model=List[LiquidityPosition])
async def list_liquidity_positions(limit: int = 30):
    """List historical liquidity positions."""
    return positions[-limit:]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
