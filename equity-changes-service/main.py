"""Vimbai Equity Changes Service - Track equity movements and shareholder changes. Port: 8366"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "equity-changes-service"
PORT = int(os.getenv("PORT", "8366"))
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


class EquityTransactionType(str, Enum):
    ISSUANCE = "issuance"
    BUYBACK = "buyback"
    DIVIDEND = "dividend"
    SPLIT = "split"
    TRANSFER = "transfer"
    RETAINED = "retained_earnings"


class EquityTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    transaction_type: EquityTransactionType
    shareholder: str = ""
    shares: int = 0
    price_per_share: float = 0
    amount: float = 0
    description: str = ""
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class EquityStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    beginning_equity: float
    share_issuances: float = 0
    share_buybacks: float = 0
    dividends_paid: float = 0
    retained_earnings_change: float = 0
    other_changes: float = 0
    ending_equity: float = 0
    transactions: List[EquityTransaction] = []


_transactions: Dict[str, List[EquityTransaction]] = defaultdict(list)
_statements: Dict[str, List[EquityStatement]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/transactions", response_model=EquityTransaction)
async def create_transaction(tx: EquityTransaction):
    tx.amount = tx.shares * tx.price_per_share if tx.amount == 0 and tx.shares > 0 else tx.amount
    _transactions[tx.company_id].append(tx)
    return tx


@app.get("/transactions/{company_id}")
async def get_transactions(company_id: str, tx_type: Optional[str] = None):
    txs = _transactions.get(company_id, [])
    if tx_type:
        txs = [t for t in txs if t.transaction_type.value == tx_type]
    return {"company_id": company_id, "transactions": txs, "total": len(txs)}


@app.post("/statement", response_model=EquityStatement)
async def generate_statement(stmt: EquityStatement):
    stmt.share_issuances = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.ISSUANCE
    )
    stmt.share_buybacks = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.BUYBACK
    )
    stmt.dividends_paid = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.DIVIDEND
    )
    stmt.retained_earnings_change = sum(
        t.amount for t in stmt.transactions if t.transaction_type == EquityTransactionType.RETAINED
    )
    stmt.other_changes = sum(
        t.amount
        for t in stmt.transactions
        if t.transaction_type in (EquityTransactionType.SPLIT, EquityTransactionType.TRANSFER)
    )
    stmt.ending_equity = (
        stmt.beginning_equity
        + stmt.share_issuances
        - stmt.share_buybacks
        - stmt.dividends_paid
        + stmt.retained_earnings_change
        + stmt.other_changes
    )
    _statements[stmt.company_id].append(stmt)
    return stmt


@app.get("/statements/{company_id}")
async def get_statements(company_id: str):
    return {
        "company_id": company_id,
        "statements": _statements.get(company_id, []),
        "total": len(_statements.get(company_id, [])),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
