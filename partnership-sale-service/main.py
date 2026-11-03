"""
Vimbai Partnership Sale Service
Handles partner retirement, admission, and partnership dissolution calculations.
Port: 8340
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
_spec = importlib.util.spec_from_file_location("partnership_sale_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["partnership_sale_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "partnership-sale-service"
PORT = int(os.getenv("PORT", "8340"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Partnership Sale Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class PartnerBuyoutRequest(BaseModel):
    company_id: str
    outgoing_partner_id: str
    outgoing_partner_name: str
    capital_balance: float
    share_of_goodwill: float = 0
    share_of_reserves: float = 0
    loan_to_partner: float = 0
    agreed_payment: float
    remaining_partners: List[Dict] = []


class BuyoutResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    outgoing_partner: str
    capital_due: float
    goodwill_due: float
    reserves_due: float
    loan_due: float
    total_payable: float
    agreed_payment: float
    difference: float
    gain_or_loss_on_retirement: float
    remaining_partner_adjustments: List[Dict] = []


class GoodwillRequest(BaseModel):
    company_id: str
    partners: List[Dict]
    avg_profit: float
    years: int = 3
    rate: float = 0.15


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/retirement", response_model=BuyoutResult)
async def calculate_retirement(req: PartnerBuyoutRequest):
    result = calc.retirement(
        req.company_id,
        req.outgoing_partner_name,
        req.capital_balance,
        req.share_of_goodwill,
        req.share_of_reserves,
        req.loan_to_partner,
        req.agreed_payment,
        req.remaining_partners,
    )
    logger.info("Partner retired", company=req.company_id, partner=req.outgoing_partner_name)
    return BuyoutResult(**result)


@app.post("/goodwill", response_model=dict)
async def calculate_goodwill(req: GoodwillRequest):
    return calc.goodwill(
        req.company_id,
        req.partners,
        req.avg_profit,
        req.years,
        req.rate,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
