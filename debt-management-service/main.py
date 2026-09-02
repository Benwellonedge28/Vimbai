"""
Vimbai Debt Management Service
Loan tracking, amortization schedules, debt restructuring, and covenant monitoring.
Port: 8370
"""
import os, uuid, math
from datetime import datetime, timezone, date
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "debt-management-service"
PORT = int(os.getenv("PORT", "8370"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Debt Management Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class Loan(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; loan_name: str; lender: str
    principal: float; interest_rate: float; term_months: int
    disbursement_date: date; payment_frequency: str = "monthly"
    remaining_balance: float = 0; status: str = "active"  # active, paid, defaulted, restructured

class AmortizationScheduleItem(BaseModel):
    period: int; payment: float; principal_component: float
    interest_component: float; balance: float

class DebtSummary(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; total_debt: float; total_interest: float
    total_monthly_payments: float; debt_to_equity: float = 0
    weighted_avg_rate: float; loans: List[Dict] = []

_loans: Dict[str, List[Loan]] = {}

def _calc_payment(principal: float, rate: float, months: int) -> float:
    if rate == 0: return principal / months
    r = rate / 12
    return principal * r * (1 + r) ** months / ((1 + r) ** months - 1)

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/loans", response_model=Loan)
async def create_loan(loan: Loan):
    loan.remaining_balance = loan.principal
    _loans.setdefault(loan.company_id, []).append(loan)
    return loan

@app.get("/loans", response_model=List[Loan])
async def list_loans(company_id: str):
    return _loans.get(company_id, [])

@app.post("/loans/{loan_id}/schedule", response_model=List[AmortizationScheduleItem])
async def get_amortization_schedule(company_id: str, loan_id: str):
    loans = _loans.get(company_id, [])
    loan = next((l for l in loans if l.id == loan_id), None)
    if not loan:
        from fastapi import HTTPException; raise HTTPException(status_code=404, detail="Loan not found")
    
    payment = _calc_payment(loan.principal, loan.interest_rate, loan.term_months)
    balance = loan.principal
    schedule = []
    
    for i in range(1, loan.term_months + 1):
        interest = balance * (loan.interest_rate / 12)
        principal_comp = payment - interest
        balance -= principal_comp
        schedule.append(AmortizationScheduleItem(
            period=i, payment=round(payment, 2),
            principal_component=round(principal_comp, 2),
            interest_component=round(interest, 2),
            balance=round(max(balance, 0), 2)
        ))
    return schedule

@app.get("/summary", response_model=DebtSummary)
async def get_debt_summary(company_id: str, equity: float = 0):
    loans = _loans.get(company_id, [])
    total_debt = sum(l.remaining_balance for l in loans)
    total_monthly = sum(_calc_payment(l.principal, l.interest_rate, l.term_months) for l in loans)
    total_interest = sum(_calc_payment(l.principal, l.interest_rate, l.term_months) * l.term_months - l.principal for l in loans)
    weighted_rate = sum(l.interest_rate * l.remaining_balance for l in loans) / total_debt if total_debt else 0
    d2e = total_debt / equity if equity else 0
    
    return DebtSummary(
        company_id=company_id, total_debt=round(total_debt, 2),
        total_interest=round(total_interest, 2), total_monthly_payments=round(total_monthly, 2),
        debt_to_equity=round(d2e, 4), weighted_avg_rate=round(weighted_rate, 4),
        loans=[{"id": l.id, "name": l.loan_name, "balance": l.remaining_balance, "rate": l.interest_rate, "status": l.status} for l in loans]
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
