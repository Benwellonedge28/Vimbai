"""
FinAcc Payroll Accounting Service
Handles payroll calculations and journal entries.
"""

import os
import uuid
from datetime import datetime
from typing:Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "payroll-accounting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8138"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Payroll Accounting Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal FinAcc service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Payroll accounting service"}


@app.post("/net-pay")
async def calculate_net_pay(
    gross_salary: float,
    tax_deducted: float = 0,
    national_insurance: float = 0,
    pension_contribution: float = 0,
    other_deductions: float = 0
):
    """Calculate employee net pay."""
    total_deductions = tax_deducted + national_insurance + pension_contribution + other_deductions
    net_pay = gross_salary - total_deductions

    return {
        "gross_salary": gross_salary,
        "deductions": {
            "tax_deducted": tax_deducted,
            "national_insurance": national_insurance,
            "pension_contribution": pension_contribution,
            "other_deductions": other_deductions,
            "total_deductions": total_deductions
        },
        "net_pay": round(net_pay, 2)
    }


@app.post("/employer-cost")
async def calculate_employer_cost(
    gross_salary: float,
    employer_ni: float = 0,
    pension_contribution: float = 0,
    benefits_cost: float = 0
):
    """Calculate total employer cost."""
    total_cost = gross_salary + employer_ni + pension_contribution + benefits_cost
    oncosts_ratio = (total_cost - gross_salary) / gross_salary if gross_salary != 0 else 0

    return {
        "gross_salary": gross_salary,
        "employer_ni": employer_ni,
        "pension_contribution": pension_contribution,
        "benefits_cost": benefits_cost,
        "total_employer_cost": round(total_cost, 2),
        "oncosts_percentage": round(oncosts_ratio * 100, 2)
    }


@app.post("/journal-entry")
async def generate_payroll_journal(
    gross_salary: float,
    tax_deducted: float,
    ni_employee: float,
    ni_employer: float,
    pension_employee: float,
    pension_employer: float,
    department: str = "Production"
):
    """Generate payroll journal entries."""
    total_deductions = tax_deducted + ni_employee + pension_employee
    net_pay = gross_salary - total_deductions

    # Debit entries (expenses)
    debits = [
        {"account": f"Wages & Salaries - {department}", "amount": gross_salary}
    ]

    # Credit entries (liabilities)
    credits = [
        {"account": "Tax Control Account", "amount": tax_deducted},
        {"account": "NI Control Account - Employee", "amount": ni_employee},
        {"account": "NI Control Account - Employer", "amount": ni_employer},
        {"account": "Pension Control - Employee", "amount": pension_employee},
        {"account": "Pension Control - Employer", "amount": pension_employer},
        {"account": "Bank/Cash", "amount": net_pay}
    ]

    return {
        "payroll_journal": {
            "gross_salary": gross_salary,
            "date": datetime.utcnow().isoformat()
        },
        "debit_entries": debits,
        "credit_entries": credits,
        "total_debits": gross_salary,
        "total_credits": sum(c["amount"] for c in credits),
        "net_pay": net_pay
    }


@app.post("/wage-slip")
async def generate_wage_slip(
    employee_name: str,
    employee_id: str,
    period: str,
    basic_salary: float,
    overtime_pay: float = 0,
    bonus: float = 0,
    commission: float = 0,
    tax_deducted: float = 0,
    national_insurance: float = 0,
    pension_employee: float = 0,
    other_deductions: float = 0
):
    """Generate detailed wage slip."""
    gross_pay = basic_salary + overtime_pay + bonus + commission
    total_deductions = tax_deducted + national_insurance + pension_employee + other_deductions
    net_pay = gross_pay - total_deductions

    return {
        "wage_slip": {
            "employee_name": employee_name,
            "employee_id": employee_id,
            "period": period,
            "earnings": {
                "basic_salary": basic_salary,
                "overtime_pay": overtime_pay,
                "bonus": bonus,
                "commission": commission,
                "gross_pay": gross_pay
            },
            "deductions": {
                "tax_deducted": tax_deducted,
                "national_insurance": national_insurance,
                "pension_employee": pension_employee,
                "other_deductions": other_deductions,
                "total_deductions": total_deductions
            },
            "net_pay": round(net_pay, 2)
        }
    }


@app.post("/batch-payroll")
async def batch_payroll_calculation(employees: List[dict]):
    """Calculate payroll for multiple employees."""
    results = []
    total_gross = 0
    total_net = 0
    total_tax = 0
    total_ni = 0

    for emp in employees:
        gross = emp.get("gross_salary", 0)
        tax = emp.get("tax_deducted", 0)
        ni = emp.get("national_insurance", 0)
        pension = emp.get("pension_contribution", 0)
        net = gross - tax - ni - pension - emp.get("other_deductions", 0)

        results.append({
            "employee_id": emp.get("employee_id"),
            "employee_name": emp.get("employee_name"),
            "gross": gross,
            "net": round(net, 2)
        })

        total_gross += gross
        total_net += net
        total_tax += tax
        total_ni += ni

    return {
        "employee_pays": results,
        "summary": {
            "total_gross": round(total_gross, 2),
            "total_tax": round(total_tax, 2),
            "total_ni": round(total_ni, 2),
            "total_net": round(total_net, 2)
        }
    }


@app.post("/journal-for-double-entry")
async def generate_payroll_journal_for_double_entry(
    employees: List[dict],
    department: str = "Admin"
):
    """Generate consolidated payroll journal for double-entry posting."""
    total_gross = sum(e.get("gross_salary", 0) for e in employees)
    total_tax = sum(e.get("tax_deducted", 0) for e in employees)
    total_ni_employee = sum(e.get("national_insurance", 0) for e in employees)
    total_pension_employee = sum(e.get("pension_contribution", 0) for e in employees)
    total_net = sum(e.get("net_pay", 0) for e in employees)

    # Journal entry
    journal = {
        "date": datetime.utcnow().isoformat(),
        "description": f"Payroll for {len(employees)} employees - {department}",
        "debit": {
            f"Wages & Salaries Expense - {department}": total_gross
        },
        "credit": {
            "Tax Payable": total_tax,
            "NI Payable - Employee": total_ni_employee,
            "NI Payable - Employer": 0,  # To be calculated
            "Pension Payable - Employee": total_pension_employee,
            "Pension Payable - Employer": 0,  # To be calculated
            "Bank/Cash": total_net
        }
    }

    return journal


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
