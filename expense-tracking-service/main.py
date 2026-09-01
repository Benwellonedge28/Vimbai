"""Vimbai Expense Tracking Service - Track and categorize business expenses. Port: 8348"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "expense-tracking-service"
PORT = int(os.getenv("PORT", "8348"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Expense Tracking Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="expense-tracking-service", instrument_app=app)
except ImportError:
    TRACER = None

class ExpenseCategory(str, Enum):
    TRAVEL = "travel"; MEALS = "meals"; OFFICE = "office"; UTILITIES = "utilities"; SOFTWARE = "software"; EQUIPMENT = "equipment"; PROFESSIONAL = "professional"; MARKETING = "marketing"; OTHER = "other"

class ExpenseStatus(str, Enum):
    PENDING = "pending"; APPROVED = "approved"; REJECTED = "rejected"; REIMBURSED = "reimbursed"

class Expense(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    employee_id: str
    category: ExpenseCategory
    amount: float
    currency: str = "USD"
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    description: str = ""
    vendor: str = ""
    receipt_url: str = ""
    status: ExpenseStatus = ExpenseStatus.PENDING
    approved_by: str = ""
    project_code: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

_expenses: Dict[str, List[Expense]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/expenses", response_model=Expense)
async def create_expense(expense: Expense):
    _expenses[expense.company_id].append(expense)
    return expense

@app.get("/expenses/{company_id}")
async def get_expenses(company_id: str, category: Optional[str] = None, status_filter: Optional[str] = None, limit: int = 100):
    expenses = _expenses.get(company_id, [])
    if category: expenses = [e for e in expenses if e.category.value == category]
    if status_filter: expenses = [e for e in expenses if e.status.value == status_filter]
    return {"company_id": company_id, "expenses": expenses[-limit:], "total": len(expenses)}

@app.put("/expenses/{expense_id}/approve")
async def approve_expense(expense_id: str, approver: str):
    for expenses in _expenses.values():
        for e in expenses:
            if e.id == expense_id:
                e.status = ExpenseStatus.APPROVED
                e.approved_by = approver
                return {"id": expense_id, "status": "approved"}
    raise HTTPException(status_code=404, detail="Expense not found")

@app.put("/expenses/{expense_id}/reject")
async def reject_expense(expense_id: str, reason: str = ""):
    for expenses in _expenses.values():
        for e in expenses:
            if e.id == expense_id:
                e.status = ExpenseStatus.REJECTED
                return {"id": expense_id, "status": "rejected"}
    raise HTTPException(status_code=404, detail="Expense not found")

@app.get("/summary/{company_id}")
async def expense_summary(company_id: str):
    expenses = _expenses.get(company_id, [])
    by_category = defaultdict(float)
    by_status = defaultdict(int)
    total = 0
    for e in expenses:
        by_category[e.category.value] += e.amount
        by_status[e.status.value] += 1
        total += e.amount
    return {"company_id": company_id, "total_expenses": len(expenses), "total_amount": total, "by_category": dict(by_category), "by_status": dict(by_status)}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
