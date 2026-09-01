"""Vimbai Household Finance Service - Personal/household financial management. Port: 8368"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "household-finance-service"
PORT = int(os.getenv("PORT", "8368"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Household Finance Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="household-finance-service", instrument_app=app)
except ImportError:
    TRACER = None

class HouseholdMember(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    role: str = "member"  # head, spouse, child, dependent
    income: float = 0

class HouseholdExpense(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    category: str
    amount: float
    frequency: str = "monthly"  # weekly, monthly, annual, one_time
    description: str = ""

class HouseholdBudget(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    household_name: str
    members: List[HouseholdMember] = []
    expenses: List[HouseholdExpense] = []
    total_income: float = 0
    total_expenses: float = 0
    savings: float = 0
    savings_rate: float = 0

_budgets: Dict[str, HouseholdBudget] = {}

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/budgets", response_model=HouseholdBudget)
async def create_budget(budget: HouseholdBudget):
    budget.total_income = sum(m.income for m in budget.members)
    budget.total_expenses = sum(e.amount for e in budget.expenses)
    budget.savings = budget.total_income - budget.total_expenses
    budget.savings_rate = (budget.savings / max(1, budget.total_income)) * 100
    _budgets[budget.id] = budget
    return budget

@app.get("/budgets/{budget_id}")
async def get_budget(budget_id: str):
    if budget_id not in _budgets: raise HTTPException(status_code=404, detail="Budget not found")
    return _budgets[budget_id]

@app.put("/budgets/{budget_id}/expenses")
async def add_expense(budget_id: str, expense: HouseholdExpense):
    if budget_id not in _budgets: raise HTTPException(status_code=404, detail="Budget not found")
    budget = _budgets[budget_id]
    budget.expenses.append(expense)
    budget.total_expenses = sum(e.amount for e in budget.expenses)
    budget.savings = budget.total_income - budget.total_expenses
    budget.savings_rate = (budget.savings / max(1, budget.total_income)) * 100
    return {"budget_id": budget_id, "savings": budget.savings, "savings_rate": budget.savings_rate}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
