"""
Vimbai Pension Accounting Service
IAS 19 defined benefit pension plan accounting with actuarial calculations.
Port: 8393
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

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
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/calculate", response_model=PensionResult)
async def calculate_pension(req: PensionRequest):
    dbo_end = (
        req.obligation_beginning
        + req.service_cost
        + req.interest_cost
        + req.actuarial_gain_loss
        + req.past_service_cost
        - req.benefits_paid
    )

    actual_return = req.actual_return or req.expected_return
    plan_assets_end = req.plan_assets_beginning + req.contributions + actual_return - req.benefits_paid_from_assets

    funded = plan_assets_end - dbo_end
    net_liability = max(-funded, 0)
    net_asset = max(funded, 0)

    net_interest = req.interest_cost - req.expected_return
    remeasurements = req.actuarial_gain_loss + (actual_return - req.expected_return)
    total_expense = req.service_cost + net_interest + req.past_service_cost

    return PensionResult(
        company_id=req.company_id,
        plan_name=req.plan_name,
        fiscal_year=req.fiscal_year,
        dbo_end=round(dbo_end, 2),
        plan_assets_end=round(plan_assets_end, 2),
        funded_position=round(funded, 2),
        net_defined_benefit_liability=round(net_liability, 2),
        current_service_cost=round(req.service_cost, 2),
        interest_cost=round(req.interest_cost, 2),
        net_interest=round(net_interest, 2),
        remeasurements=round(remeasurements, 2),
        total_pension_expense=round(total_expense, 2),
        oci_actuarial_gain_loss=round(req.actuarial_gain_loss, 2),
        balance_sheet_entry={
            "defined_benefit_asset": round(net_asset, 2),
            "defined_benefit_liability": round(net_liability, 2),
        },
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
