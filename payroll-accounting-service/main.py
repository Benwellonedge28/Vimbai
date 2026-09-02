"""
Vimbai Payroll Accounting Service
Payroll calculation, statutory deductions, and journal entry generation.
Port: 8338
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "payroll-accounting-service"
PORT = int(os.getenv("PORT", "8338"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Payroll Accounting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class EmployeePayroll(BaseModel):
    employee_id: str
    employee_name: str
    gross_salary: float
    paye_rate: float = 0.25
    nassa_rate: float = 0.03
    pension_rate: float = 0.05
    medical_aid: float = 0
    other_deductions: float = 0


class PayrollRequest(BaseModel):
    company_id: str
    period: str
    employees: List[EmployeePayroll]


class PayrollResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    total_gross: float
    total_net: float
    total_paye: float
    total_nassa: float
    total_pension: float
    total_medical: float
    total_other_deductions: float
    employer_pension: float
    employer_nassa: float
    total_cost_to_company: float
    employee_count: int
    journal_entries: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/process", response_model=PayrollResult)
async def process_payroll(req: PayrollRequest):
    total_gross = 0
    total_net = 0
    total_paye = 0
    total_nassa = 0
    total_pension = 0
    total_medical = 0
    total_other = 0

    for emp in req.employees:
        paye = emp.gross_salary * emp.paye_rate
        nassa = emp.gross_salary * emp.nassa_rate
        pension = emp.gross_salary * emp.pension_rate
        total_deductions = paye + nassa + pension + emp.medical_aid + emp.other_deductions
        net = emp.gross_salary - total_deductions

        total_gross += emp.gross_salary
        total_net += net
        total_paye += paye
        total_nassa += nassa
        total_pension += pension
        total_medical += emp.medical_aid
        total_other += emp.other_deductions

    employer_pension = total_gross * 0.07
    employer_nassa = total_gross * 0.03
    total_cost = total_gross + employer_pension + employer_nassa

    journal_entries = [
        {"account": "Salaries Expense", "debit": round(total_gross, 2), "credit": 0},
        {"account": "Pension Expense", "debit": round(employer_pension, 2), "credit": 0},
        {"account": "NSSA Expense", "debit": round(employer_nassa, 2), "credit": 0},
        {"account": "PAYE Payable", "debit": 0, "credit": round(total_paye, 2)},
        {"account": "NSSA Payable", "debit": 0, "credit": round(total_nassa + employer_nassa, 2)},
        {"account": "Pension Payable", "debit": 0, "credit": round(total_pension + employer_pension, 2)},
        {"account": "Medical Aid Payable", "debit": 0, "credit": round(total_medical, 2)},
        {"account": "Salaries Payable", "debit": 0, "credit": round(total_net, 2)},
    ]

    return PayrollResult(
        company_id=req.company_id,
        period=req.period,
        total_gross=round(total_gross, 2),
        total_net=round(total_net, 2),
        total_paye=round(total_paye, 2),
        total_nassa=round(total_nassa, 2),
        total_pension=round(total_pension, 2),
        total_medical=round(total_medical, 2),
        total_other_deductions=round(total_other, 2),
        employer_pension=round(employer_pension, 2),
        employer_nassa=round(employer_nassa, 2),
        total_cost_to_company=round(total_cost, 2),
        employee_count=len(req.employees),
        journal_entries=journal_entries,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
