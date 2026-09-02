"""Vimbai Sovereign Treasury Service - Sovereign/national treasury management. Port: 8370"""

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

    TRACER = setup_tracing(service_name="sovereign-treasury-service", instrument_app=app)
except ImportError:
    TRACER = None


class SovereignAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    country: str
    account_type: str  # consolidated_revenue, stabilization_fund, debt_management, foreign_reserves
    balance: float
    currency: str = "USD"
    description: str = ""


class SovereignDebt(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    country: str
    instrument: str  # treasury_bill, government_bond, external_debt, eurobond
    principal: float
    interest_rate: float
    maturity_date: datetime
    outstanding: float
    currency: str = "USD"


class FiscalPosition(BaseModel):
    country: str
    fiscal_year: str
    total_revenue: float
    total_expenditure: float
    fiscal_deficit: float = 0
    deficit_to_gdp: float = 0
    total_debt: float = 0
    debt_to_gdp: float = 0
    foreign_reserves: float = 0
    months_import_cover: float = 0


_accounts: Dict[str, List[SovereignAccount]] = defaultdict(list)
_debts: Dict[str, List[SovereignDebt]] = defaultdict(list)
_positions: Dict[str, FiscalPosition] = {}


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/accounts")
async def create_account(account: SovereignAccount):
    _accounts[account.country].append(account)
    return {"id": account.id, "type": account.account_type, "balance": account.balance}


@app.get("/accounts/{country}")
async def get_accounts(country: str):
    accounts = _accounts.get(country, [])
    return {"country": country, "accounts": accounts, "total_balance": sum(a.balance for a in accounts)}


@app.post("/debt")
async def register_debt(debt: SovereignDebt):
    _debts[debt.country].append(debt)
    return {"id": debt.id, "instrument": debt.instrument, "outstanding": debt.outstanding}


@app.get("/debt/{country}")
async def get_debt(country: str):
    debts = _debts.get(country, [])
    total = sum(d.outstanding for d in debts)
    return {"country": country, "debts": debts, "total_debt": total, "instruments": len(debts)}


@app.post("/fiscal-position")
async def set_fiscal_position(pos: FiscalPosition):
    pos.fiscal_deficit = pos.total_expenditure - pos.total_revenue
    _positions[pos.country] = pos
    return pos


@app.get("/fiscal-position/{country}")
async def get_fiscal_position(country: str):
    if country not in _positions:
        raise HTTPException(status_code=404, detail="No fiscal position found")
    return _positions[country]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
