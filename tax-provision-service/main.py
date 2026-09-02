"""
Vimbai Tax Provision Service
ASC 740 tax provision calculations with deferred tax assets/liabilities.
Port: 8375
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "tax-provision-service"
PORT = int(os.getenv("PORT", "8375"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Tax Provision Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class TemporaryDifference(BaseModel):
    description: str
    book_amount: float
    tax_amount: float
    type: str = "taxable"  # taxable, deductible


class TaxProvisionRequest(BaseModel):
    company_id: str
    fiscal_year: int
    pre_tax_income: float
    statutory_rate: float = 0.25
    permanent_differences: float = 0
    temp_differences: List[TemporaryDifference] = []
    loss_carryforward: float = 0
    valuation_allowance: float = 0


class TaxProvisionResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    current_tax_expense: float
    deferred_tax_expense: float
    total_tax_expense: float
    effective_rate: float
    deferred_tax_assets: float
    deferred_tax_liabilities: float
    net_deferred_tax: float
    valuation_allowance: float
    loss_carryforward_utilized: float
    statutory_rate: float = 0.25


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/calculate", response_model=TaxProvisionResult)
async def calculate_provision(req: TaxProvisionRequest):
    taxable_income = req.pre_tax_income + req.permanent_differences

    for td in req.temp_differences:
        diff = td.book_amount - td.tax_amount
        if td.type == "taxable":
            taxable_income += diff
        else:
            taxable_income -= diff

    if req.loss_carryforward > 0:
        utilized = min(req.loss_carryforward, max(taxable_income, 0))
        taxable_income -= utilized
    else:
        utilized = 0

    current_tax = max(taxable_income, 0) * req.statutory_rate

    dta = 0
    dtl = 0
    for td in req.temp_differences:
        diff = td.book_amount - td.tax_amount
        if td.type == "deductible":
            dta += abs(diff) * req.statutory_rate
        else:
            dtl += abs(diff) * req.statutory_rate

    net_deferred = dta - dtl - req.valuation_allowance
    deferred_tax = dtl - dta
    total_tax = current_tax + deferred_tax
    effective_rate = total_tax / req.pre_tax_income if req.pre_tax_income else 0

    return TaxProvisionResult(
        company_id=req.company_id,
        fiscal_year=req.fiscal_year,
        current_tax_expense=round(current_tax, 2),
        deferred_tax_expense=round(deferred_tax, 2),
        total_tax_expense=round(total_tax, 2),
        effective_rate=round(effective_rate, 4),
        deferred_tax_assets=round(dta, 2),
        deferred_tax_liabilities=round(dtl, 2),
        net_deferred_tax=round(net_deferred, 2),
        valuation_allowance=req.valuation_allowance,
        loss_carryforward_utilized=round(utilized, 2),
        statutory_rate=req.statutory_rate,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
