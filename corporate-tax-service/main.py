"""
Vimbai Corporate Tax Service
Corporate income tax calculation with adjustments, credits, and installment planning.
Port: 8399
"""

import importlib.util
import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "corporate-tax-service"
PORT = int(os.getenv("PORT", "8399"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("corporate_tax_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["corporate_tax_calc"] = calc
_spec.loader.exec_module(calc)

app = FastAPI(title="Vimbai Corporate Tax Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class TaxAdjustment(BaseModel):
    description: str
    amount: float
    type: str = "addition"  # addition, deduction


class TaxCredit(BaseModel):
    description: str
    amount: float
    carryforward_years: int = 0


class CorporateTaxRequest(BaseModel):
    company_id: str
    fiscal_year: int
    accounting_profit: float
    statutory_rate: float = 0.25
    adjustments: List[TaxAdjustment] = []
    credits: List[TaxCredit] = []
    estimated_payments: float = 0


class CorporateTaxResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    accounting_profit: float
    taxable_income: float
    tax_before_credits: float
    total_credits: float
    net_tax_liability: float
    effective_rate: float
    estimated_payments: float
    balance_due: float
    installment_schedule: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


def _reference(payload):
    """The original Python calculation, kept as the oracle.

    The adjustment type is spelled "addition"; anything else subtracts. That is the
    behaviour this endpoint has always had, so a caller sending "add" gets a
    deduction and always has.
    """
    taxable = payload["accounting_profit"]
    for adj in payload["adjustments"]:
        if adj["type"] == "addition":
            taxable += adj["amount"]
        else:
            taxable -= adj["amount"]
    taxable = max(taxable, 0)

    tax_before = taxable * payload["statutory_rate"]
    total_credits = sum(c["amount"] for c in payload["credits"])
    net_tax = max(tax_before - total_credits, 0)
    effective = net_tax / payload["accounting_profit"] if payload["accounting_profit"] else 0
    balance = max(net_tax - payload["estimated_payments"], 0)

    installments = []
    for q in range(1, 5):
        q_payment = net_tax / 4
        installments.append(
            {
                "quarter": q,
                "due_date": f"{payload['fiscal_year']}-{q * 3:02d}-15",
                "amount": round(q_payment, 2),
            }
        )

    return dict(
        company_id=payload["company_id"],
        fiscal_year=payload["fiscal_year"],
        accounting_profit=round(payload["accounting_profit"], 2),
        taxable_income=round(taxable, 2),
        tax_before_credits=round(tax_before, 2),
        total_credits=round(total_credits, 2),
        net_tax_liability=round(net_tax, 2),
        effective_rate=round(effective, 4),
        estimated_payments=round(payload["estimated_payments"], 2),
        balance_due=round(balance, 2),
        installment_schedule=installments,
    )


@app.post("/calculate", response_model=CorporateTaxResult)
async def calculate_tax(req: CorporateTaxRequest):
    logger.info("Calculating corporate tax", company=req.company_id, year=req.fiscal_year)
    result = calc.run(req.model_dump(mode="json"), _reference)
    result.setdefault("id", str(uuid.uuid4()))
    return CorporateTaxResult(**result)


# Backward-compatible /compute endpoint
class ComputeTaxReq(BaseModel):
    company_id: str
    tax_year: int
    revenue: float
    deductible_expenses: float
    capital_allowances: float = 0
    tax_rate: float = 25.0
    credits: float = 0


@app.post("/compute")
async def compute_tax(req: ComputeTaxReq):
    taxable = max(req.revenue - req.deductible_expenses - req.capital_allowances, 0)
    tax_owed = taxable * (req.tax_rate / 100)
    net = max(tax_owed - req.credits, 0)
    return {
        "taxable_income": round(taxable, 2),
        "tax_owed": round(tax_owed, 2),
        "net_tax_liability": round(net, 2),
        "effective_rate": round(net / req.revenue * 100, 2) if req.revenue else 0,
    }


@app.post("/provision/{company_id}")
async def provisional_tax(company_id: str, tax_year: int, annual_estimate: float, tax_rate: float = 25.0):
    annual_tax = annual_estimate * (tax_rate / 100)
    quarterly = annual_tax / 4
    from datetime import datetime

    due_dates = [f"{tax_year}-{m}-15" for m in (3, 6, 9, 12)]
    return {
        "company_id": company_id,
        "annual_tax_estimate": round(annual_tax, 2),
        "quarterly_payment": round(quarterly, 2),
        "due_dates": due_dates,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
