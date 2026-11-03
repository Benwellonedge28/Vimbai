"""
Vimbai VAT Reporting Service
VAT calculation, returns, and compliance reporting.
Port: 8351
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
_spec = importlib.util.spec_from_file_location("vat_reporting_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["vat_reporting_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "vat-reporting-service"
PORT = int(os.getenv("PORT", "8351"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai VAT Reporting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

VAT_STANDARD_RATE = 0.15
VAT_ZERO_RATE = 0.0
VAT_EXEMPT = "exempt"


class VATTransaction(BaseModel):
    description: str
    amount: float
    vat_rate: float = VAT_STANDARD_RATE
    transaction_type: str = "standard"  # standard, zero_rated, exempt


class VATReturnRequest(BaseModel):
    company_id: str
    tax_period: str  # e.g. "2026-Q1"
    input_transactions: List[VATTransaction] = []  # purchases
    output_transactions: List[VATTransaction] = []  # sales


class VATReturn(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    tax_period: str
    total_output_vat: float
    total_input_vat: float
    net_vat_payable: float
    net_vat_refundable: float
    standard_rated_sales: float
    zero_rated_sales: float
    exempt_sales: float
    standard_rated_purchases: float
    vat_due_to_authority: float
    submission_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).isoformat())


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/prepare-return", response_model=VATReturn)
async def prepare_vat_return(req: VATReturnRequest):
    result = calc.prepare_return(
        req.company_id,
        req.tax_period,
        [t.model_dump() for t in req.input_transactions],
        [t.model_dump() for t in req.output_transactions],
    )
    logger.info("VAT return prepared", company=req.company_id, period=req.tax_period)
    return VATReturn(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
