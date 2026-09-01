"""Vimbai Cash Optimization Service - Optimize cash allocation across accounts. Port: 8324"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "cash-optimization-service"
PORT = int(os.getenv("PORT", "8324"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Cash Optimization Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="cash-optimization-service", instrument_app=app)
except ImportError:
    TRACER = None

class AccountType(str, Enum):
    OPERATING = "operating"; RESERVE = "reserve"; INVESTMENT = "investment"; TAX = "tax"; PAYROLL = "payroll"

class CashAccount(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    account_name: str
    account_type: AccountType
    balance: float
    min_required: float = 0
    interest_rate: float = 0
    currency: str = "USD"

class OptimizationSuggestion(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    from_account: str
    to_account: str
    amount: float
    reason: str
    expected_benefit: float = 0  # annual benefit
    priority: str = "medium"  # low, medium, high

_accounts: Dict[str, List[CashAccount]] = defaultdict(list)
_suggestions: Dict[str, List[OptimizationSuggestion]] = defaultdict(list)

def optimize(company_id: str) -> List[OptimizationSuggestion]:
    accounts = _accounts.get(company_id, [])
    suggestions = []
    if not accounts:
        return suggestions
    
    # Find excess cash in operating accounts
    for acc in accounts:
        excess = acc.balance - acc.min_required
        if excess > 10000 and acc.account_type == AccountType.OPERATING:
            # Find best investment account
            inv_accounts = [a for a in accounts if a.account_type == AccountType.INVESTMENT]
            if inv_accounts:
                best = max(inv_accounts, key=lambda a: a.interest_rate)
                annual_benefit = excess * best.interest_rate
                suggestions.append(OptimizationSuggestion(
                    company_id=company_id, from_account=acc.account_name, to_account=best.account_name,
                    amount=excess, reason=f"Move excess operating cash to higher-yield investment account ({best.interest_rate*100:.1f}% APR)",
                    expected_benefit=annual_benefit, priority="high"
                ))
        
        # Check reserve accounts with excess
        if excess > 50000 and acc.account_type == AccountType.RESERVE:
            inv_accounts = [a for a in accounts if a.account_type == AccountType.INVESTMENT and a.interest_rate > acc.interest_rate]
            if inv_accounts:
                best = max(inv_accounts, key=lambda a: a.interest_rate)
                rate_diff = best.interest_rate - acc.interest_rate
                annual_benefit = excess * rate_diff
                suggestions.append(OptimizationSuggestion(
                    company_id=company_id, from_account=acc.account_name, to_account=best.account_name,
                    amount=excess, reason=f"Transfer to higher-yield account for {rate_diff*100:.1f}% rate improvement",
                    expected_benefit=annual_benefit, priority="medium"
                ))
    
    # Check for underfunded accounts
    for acc in accounts:
        if acc.balance < acc.min_required and acc.account_type in (AccountType.RESERVE, AccountType.TAX, AccountType.PAYROLL):
            deficit = acc.min_required - acc.balance
            operating = [a for a in accounts if a.account_type == AccountType.OPERATING and a.balance > a.min_required + deficit]
            if operating:
                source = max(operating, key=lambda a: a.balance - a.min_required)
                suggestions.append(OptimizationSuggestion(
                    company_id=company_id, from_account=source.account_name, to_account=acc.account_name,
                    amount=deficit, reason=f"Top up {acc.account_type.value} account to meet minimum requirement",
                    expected_benefit=0, priority="high"
                ))
    
    return suggestions

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/accounts")
async def add_account(account: CashAccount):
    _accounts[account.company_id].append(account)
    return {"id": account.id, "account_name": account.account_name, "balance": account.balance}

@app.get("/accounts/{company_id}")
async def get_accounts(company_id: str):
    return {"company_id": company_id, "accounts": _accounts.get(company_id, [])}

@app.post("/optimize/{company_id}")
async def run_optimization(company_id: str):
    suggestions = optimize(company_id)
    _suggestions[company_id] = suggestions
    total_benefit = sum(s.expected_benefit for s in suggestions)
    return {"company_id": company_id, "suggestions": suggestions, "total_count": len(suggestions), "potential_annual_benefit": total_benefit}

@app.get("/suggestions/{company_id}")
async def get_suggestions(company_id: str):
    return {"company_id": company_id, "suggestions": _suggestions.get(company_id, [])}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
