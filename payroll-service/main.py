"""
Payroll Service
Port: 8354
Payroll processing and management
"""

from datetime import date, datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Payroll Service", version="1.0.0")


class EmployeePay(BaseModel):
    employee_id: str
    gross_pay: float
    deductions: Dict[str, float]
    taxes: Dict[str, float]
    net_pay: float


class PayrollRunRequest(BaseModel):
    company_id: str
    pay_period_start: date
    pay_period_end: date
    employees: List[Dict[str, Any]]
    deductions: Dict[str, float]


class PayrollRunResponse(BaseModel):
    payroll_id: str
    company_id: str
    pay_period: Dict[str, date]
    total_gross: float
    total_deductions: float
    total_taxes: float
    total_net: float
    employee_pays: List[EmployeePay]
    status: str


class PayrollTaxRequest(BaseModel):
    company_id: str
    employee_id: str
    gross_wages: float
    ytd_wages: float


class PayrollTaxResponse(BaseModel):
    federal_withholding: float
    state_withholding: float
    social_security: float
    medicare: float
    futa: float
    total_taxes: float


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "payroll", "version": "1.0.0"}


@app.post("/run", response_model=PayrollRunResponse)
async def run_payroll(request: PayrollRunRequest):
    logger.info("Running payroll", company=request.company_id, employees=len(request.employees))

    employee_pays = []
    total_gross = 0.0
    total_deductions = 0.0
    total_taxes = 0.0

    for emp in request.employees:
        gross = emp.get("gross_pay", 5000)
        taxes = {
            "federal": gross * 0.15,
            "state": gross * 0.05,
            "social_security": gross * 0.062,
            "medicare": gross * 0.0145,
        }
        deductions = {k: v for k, v in request.deductions.items()}
        net = gross - sum(taxes.values()) - sum(deductions.values())

        employee_pays.append(
            EmployeePay(
                employee_id=emp.get("employee_id"),
                gross_pay=gross,
                deductions=deductions,
                taxes=taxes,
                net_pay=round(net, 2),
            )
        )
        total_gross += gross
        total_taxes += sum(taxes.values())
        total_deductions += sum(deductions.values())

    return PayrollRunResponse(
        payroll_id=f"PR-{datetime.now().strftime('%Y%m%d')}",
        company_id=request.company_id,
        pay_period={"start": request.pay_period_start, "end": request.pay_period_end},
        total_gross=round(total_gross, 2),
        total_deductions=round(total_deductions, 2),
        total_taxes=round(total_taxes, 2),
        total_net=round(total_gross - total_deductions - total_taxes, 2),
        employee_pays=employee_pays,
        status="processed",
    )


@app.post("/taxes", response_model=PayrollTaxResponse)
async def calculate_payroll_taxes(request: PayrollTaxRequest):
    logger.info("Calculating payroll taxes", company=request.company_id, employee=request.employee_id)

    federal = request.gross_wages * 0.15
    state = request.gross_wages * 0.05
    ss = min(request.gross_wages * 0.062, 10080.60)
    medicare = request.gross_wages * 0.0145
    futa = min(request.gross_wages * 0.006, 42.30)

    return PayrollTaxResponse(
        federal_withholding=round(federal, 2),
        state_withholding=round(state, 2),
        social_security=round(ss, 2),
        medicare=round(medicare, 2),
        futa=round(futa, 2),
        total_taxes=round(federal + state + ss + medicare + futa, 2),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8354)
