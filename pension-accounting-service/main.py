"""
Vimbai Pension Accounting Service
IAS 19 defined benefit pension plan accounting with actuarial calculations.
Port: 8393
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
_spec = importlib.util.spec_from_file_location("pension_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["pension_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "pension-accounting-service"
PORT = int(os.getenv("PORT", "8393"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Pension Accounting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class PensionRequest(BaseModel):
    company_id: str
    fiscal_year: int
    plan_name: str
    obligation_beginning: float
    service_cost: float
    interest_cost: float
    benefits_paid: float
    actuarial_gain_loss: float = 0
    past_service_cost: float = 0
    plan_assets_beginning: float
    contributions: float
    expected_return: float
    actual_return: float = 0
    benefits_paid_from_assets: float = 0


class PensionResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    plan_name: str
    fiscal_year: int
    dbo_end: float
    plan_assets_end: float
    funded_position: float
    net_defined_benefit_liability: float
    current_service_cost: float
    interest_cost: float
    net_interest: float
    remeasurements: float
    total_pension_expense: float
    oci_actuarial_gain_loss: float
    oci_return_adjustment: float = 0
    balance_sheet_entry: Dict[str, float] = {}


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/calculate", response_model=PensionResult)
async def calculate_pension(req: PensionRequest):
    result = calc.pension_expense(
        req.company_id,
        req.fiscal_year,
        req.plan_name,
        req.obligation_beginning,
        req.service_cost,
        req.interest_cost,
        req.benefits_paid,
        req.actuarial_gain_loss,
        req.past_service_cost,
        req.plan_assets_beginning,
        req.contributions,
        req.expected_return,
        req.actual_return,
        req.benefits_paid_from_assets,
    )
    logger.info("Pension expense calculated", company=req.company_id, plan=req.plan_name)
    return PensionResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
