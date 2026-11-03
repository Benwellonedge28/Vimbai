"""
Vimbai Tax Provision Service
ASC 740 tax provision calculations with deferred tax assets/liabilities.
Port: 8375
"""

import importlib.util
import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("tax_provision_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["tax_provision_calc"] = calc
_spec.loader.exec_module(calc)

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
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/calculate", response_model=TaxProvisionResult)
async def calculate_provision(req: TaxProvisionRequest):
    result = calc.tax_provision(
        req.company_id,
        req.fiscal_year,
        req.pre_tax_income,
        req.statutory_rate,
        req.permanent_differences,
        [td.model_dump() for td in req.temp_differences],
        req.loss_carryforward,
        req.valuation_allowance,
    )
    logger.info("Tax provision calculated", company=req.company_id, year=req.fiscal_year)
    return TaxProvisionResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
