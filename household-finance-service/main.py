"""
Vimbai Household Finance Service
Personal/household budget management with income/expense tracking and net worth calculation.
Port: 8403
"""

import os
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "household-finance-service"
PORT = int(os.getenv("PORT", "8403"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Household Finance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class ExpenseCategory(str, Enum):
    HOUSING = "housing"
    FOOD = "food"
    TRANSPORT = "transport"
    EDUCATION = "education"
    HEALTHCARE = "healthcare"
    UTILITIES = "utilities"
    ENTERTAINMENT = "entertainment"
    SAVINGS = "savings"
    OTHER = "other"


class HouseholdExpense(BaseModel):
    category: ExpenseCategory
    description: str
    amount: float
    frequency: str = "monthly"


class HouseholdIncome(BaseModel):
    source: str
    amount: float
    frequency: str = "monthly"


class Asset(BaseModel):
    name: str
    value: float
    type: str = "cash"  # cash, investment, property, vehicle


class Liability(BaseModel):
    name: str
    amount: float
    type: str = "loan"  # loan, mortgage, credit_card


class HouseholdRequest(BaseModel):
    household_id: str
    period: str
    incomes: List[HouseholdIncome] = []
    expenses: List[HouseholdExpense] = []
    assets: List[Asset] = []
    liabilities: List[Liability] = []


class HouseholdResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    household_id: str
    period: str
    total_income: float
    total_expenses: float
    surplus_deficit: float
    savings_rate: float
    net_worth: float
    expense_by_category: Dict[str, float] = {}
    budget_health: str


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/analyze", response_model=HouseholdResult)
async def analyze_household(req: HouseholdRequest):
    income = sum(i.amount for i in req.incomes)
    expenses = sum(e.amount for e in req.expenses)
    surplus = income - expenses
    savings_rate = (surplus / income * 100) if income else 0

    by_cat = {}
    for e in req.expenses:
        by_cat[e.category.value] = by_cat.get(e.category.value, 0) + e.amount

    assets_total = sum(a.value for a in req.assets)
    liabilities_total = sum(l.amount for l in req.liabilities)
    net_worth = assets_total - liabilities_total

    if savings_rate >= 20:
        health = "excellent"
    elif savings_rate >= 10:
        health = "good"
    elif savings_rate >= 0:
        health = "fair"
    else:
        health = "poor"

    return HouseholdResult(
        household_id=req.household_id,
        period=req.period,
        total_income=round(income, 2),
        total_expenses=round(expenses, 2),
        surplus_deficit=round(surplus, 2),
        savings_rate=round(savings_rate, 1),
        net_worth=round(net_worth, 2),
        expense_by_category={k: round(v, 2) for k, v in by_cat.items()},
        budget_health=health,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
