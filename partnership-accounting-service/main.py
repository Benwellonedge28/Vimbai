"""
Vimbai Partnership Accounting Service
Partner capital accounts, profit sharing, and partnership financial statements.
Port: 8339
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
_spec = importlib.util.spec_from_file_location("partnership_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["partnership_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "partnership-accounting-service"
PORT = int(os.getenv("PORT", "8339"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Partnership Accounting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class Partner(BaseModel):
    partner_id: str
    name: str
    capital_contribution: float
    profit_share_pct: float
    salary: float = 0
    interest_on_capital_rate: float = 0.05
    drawings: float = 0


class PartnershipRequest(BaseModel):
    company_id: str
    period: str
    partners: List[Partner]
    net_profit: float
    interest_rate_on_capital: float = 0.05


class PartnerAccount(BaseModel):
    partner_id: str
    name: str
    opening_capital: float
    salary: float
    interest_on_capital: float
    profit_share: float
    drawings: float
    closing_capital: float


class PartnershipResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    total_salary: float
    total_interest: float
    total_profit_share: float
    residual_profit: float
    partner_accounts: List[PartnerAccount]


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/allocate", response_model=PartnershipResult)
async def allocate_profit(req: PartnershipRequest):
    result = calc.allocate(
        req.company_id,
        req.period,
        [p.model_dump() for p in req.partners],
        req.net_profit,
        req.interest_rate_on_capital,
    )
    logger.info("Profit allocated", company=req.company_id, period=req.period)
    return PartnershipResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
