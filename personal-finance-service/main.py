"""Vimbai Personal Finance Service - Individual financial planning and tracking. Port: 8369"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "personal-finance-service"
PORT = int(os.getenv("PORT", "8369"))
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
app = FastAPI(title="Vimbai Personal Finance Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="personal-finance-service", instrument_app=app)
except ImportError:
    TRACER = None


class FinancialGoal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    name: str
    target_amount: float
    current_amount: float = 0
    target_date: Optional[datetime] = None
    category: str = "savings"  # savings, debt_payoff, investment, emergency_fund
    priority: int = 3


class IncomeSource(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    source: str
    amount: float
    frequency: str = "monthly"


class DebtItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    creditor: str
    balance: float
    interest_rate: float
    min_payment: float
    type: str = "credit_card"  # credit_card, loan, mortgage, student_loan


_goals: Dict[str, List[FinancialGoal]] = defaultdict(list)
_income: Dict[str, List[IncomeSource]] = defaultdict(list)
_debts: Dict[str, List[DebtItem]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/goals")
async def create_goal(goal: FinancialGoal):
    _goals[goal.user_id].append(goal)
    return goal


@app.get("/goals/{user_id}")
async def get_goals(user_id: str):
    goals = _goals.get(user_id, [])
    return {
        "user_id": user_id,
        "goals": goals,
        "total": len(goals),
        "progress_avg": sum(g.current_amount / max(1, g.target_amount) for g in goals) / max(1, len(goals)),
    }


@app.post("/income")
async def add_income(income: IncomeSource):
    _income[income.user_id].append(income)
    return income


@app.get("/income/{user_id}")
async def get_income(user_id: str):
    sources = _income.get(user_id, [])
    total = sum(s.amount for s in sources)
    return {"user_id": user_id, "sources": sources, "total_monthly": total}


@app.post("/debts")
async def add_debt(debt: DebtItem):
    _debts[debt.user_id].append(debt)
    return debt


@app.get("/debts/{user_id}")
async def get_debts(user_id: str):
    debts = _debts.get(user_id, [])
    total = sum(d.balance for d in debts)
    total_min = sum(d.min_payment for d in debts)
    return {"user_id": user_id, "debts": debts, "total_debt": total, "total_min_payments": total_min}


@app.get("/overview/{user_id}")
async def financial_overview(user_id: str):
    income_total = sum(s.amount for s in _income.get(user_id, []))
    debt_total = sum(d.balance for d in _debts.get(user_id, []))
    goals = _goals.get(user_id, [])
    return {
        "user_id": user_id,
        "monthly_income": income_total,
        "total_debt": debt_total,
        "debt_to_income": debt_total / max(1, income_total * 12) * 100,
        "active_goals": len(goals),
        "goals_progress": sum(g.current_amount for g in goals),
        "goals_target": sum(g.target_amount for g in goals),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
