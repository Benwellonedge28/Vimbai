"""Vimbai Fund Accounting Service - Fund management and reporting. Port: 8360"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "fund-accounting-service"
PORT = int(os.getenv("PORT", "8360"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Fund Accounting Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="fund-accounting-service", instrument_app=app)
except ImportError:
    TRACER = None

class Fund(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fund_name: str
    fund_type: str = "general"  # general, restricted, endowment, project
    balance: float = 0
    income: float = 0
    expenses: float = 0
    net_assets: float = 0
    restrictions: str = ""
    manager: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

class FundTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    fund_id: str
    description: str
    amount: float
    is_income: bool
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    category: str = ""

_funds: Dict[str, List[Fund]] = defaultdict(list)
_transactions: Dict[str, List[FundTransaction]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/funds", response_model=Fund)
async def create_fund(fund: Fund):
    fund.net_assets = fund.balance + fund.income - fund.expenses
    _funds[fund.company_id].append(fund)
    return fund

@app.get("/funds/{company_id}")
async def get_funds(company_id: str):
    funds = _funds.get(company_id, [])
    return {"company_id": company_id, "funds": funds, "total_balance": sum(f.balance for f in funds), "total_net_assets": sum(f.net_assets for f in funds)}

@app.post("/transactions")
async def record_transaction(tx: FundTransaction):
    _transactions[tx.fund_id].append(tx)
    # Update fund balance
    for funds in _funds.values():
        for f in funds:
            if f.id == tx.fund_id:
                if tx.is_income:
                    f.income += tx.amount
                else:
                    f.expenses += tx.amount
                f.net_assets = f.balance + f.income - f.expenses
                break
    return {"id": tx.id, "status": "recorded"}

@app.get("/transactions/{fund_id}")
async def get_transactions(fund_id: str):
    return {"fund_id": fund_id, "transactions": _transactions.get(fund_id, []), "total": len(_transactions.get(fund_id, []))}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
