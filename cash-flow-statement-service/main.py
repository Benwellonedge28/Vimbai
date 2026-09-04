"""Vimbai Cash Flow Statement Service - Generate cash flow statements (direct/indirect). Port: 8346"""

import os
import uuid
from collections import defaultdict
from contextvars import ContextVar
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "cash-flow-statement-service"
PORT = int(os.getenv("PORT", "8346"))
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
app = FastAPI(title="Vimbai Cash Flow Statement Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="cash-flow-statement-service", instrument_app=app)
except ImportError:
    TRACER = None


class CashFlowMethod(str, Enum):
    DIRECT = "direct"
    INDIRECT = "indirect"


class CashFlowLine(BaseModel):
    description: str
    amount: float
    is_inflow: bool = True


class CashFlowStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period_start: datetime
    period_end: datetime
    method: CashFlowMethod = CashFlowMethod.INDIRECT
    operating_activities: List[CashFlowLine] = []
    investing_activities: List[CashFlowLine] = []
    financing_activities: List[CashFlowLine] = []
    net_operating: float = 0
    net_investing: float = 0
    net_financing: float = 0
    net_change: float = 0
    beginning_cash: float = 0
    ending_cash: float = 0


# Statements are stored per (Book, company): Book context comes from the
# gateway-verified X-Book-ID header; None means personal / unscoped calls.
_statements: Dict[tuple, List[CashFlowStatement]] = defaultdict(list)

_book_id_var = ContextVar("book_id", default=None)


def _statements_key(company_id: str) -> tuple:
    return (_book_id_var.get(), company_id)


def calc_net(lines: List[CashFlowLine]) -> float:
    return sum(l.amount if l.is_inflow else -l.amount for l in lines)


@app.middleware("http")
async def book_context_middleware(request, call_next):
    """Bind the gateway-verified X-Book-ID into the request-scoped contextvar."""
    token = _book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        _book_id_var.reset(token)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/generate", response_model=CashFlowStatement)
async def generate_statement(stmt: CashFlowStatement):
    stmt.net_operating = calc_net(stmt.operating_activities)
    stmt.net_investing = calc_net(stmt.investing_activities)
    stmt.net_financing = calc_net(stmt.financing_activities)
    stmt.net_change = stmt.net_operating + stmt.net_investing + stmt.net_financing
    stmt.ending_cash = stmt.beginning_cash + stmt.net_change
    _statements[_statements_key(stmt.company_id)].append(stmt)
    logger.info("cash_flow_generated", company_id=stmt.company_id, net_change=stmt.net_change, method=stmt.method.value)
    return stmt


@app.get("/latest/{company_id}")
async def get_latest(company_id: str):
    stmts = _statements.get(_statements_key(company_id), [])
    if not stmts:
        raise HTTPException(status_code=404, detail="No cash flow statements found")
    return stmts[-1]


@app.get("/history/{company_id}")
async def get_history(company_id: str):
    stmts = _statements.get(_statements_key(company_id), [])
    return {
        "company_id": company_id,
        "statements": stmts,
        "total": len(stmts),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
