"""
Vimbai Government Grants Service
IAS 20 government grant recognition, deferred income tracking, and compliance.
Port: 8408
"""

import importlib.util
import os
import sys
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("government_grants_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["government_grants_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "government-grants-service"
PORT = int(os.getenv("PORT", "8408"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Government Grants Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class GrantType(str, Enum):
    ASSET = "asset"
    INCOME = "income"


class GrantRecognition(str, Enum):
    DEFERRED_INCOME = "deferred_income"
    NET_ASSET = "net_asset"


class GrantRequest(BaseModel):
    company_id: str
    grant_name: str
    granting_authority: str = ""
    grant_amount: float
    grant_type: GrantType
    recognition_method: GrantRecognition = GrantRecognition.DEFERRED_INCOME
    asset_useful_life: int = 5  # for asset grants
    related_asset_cost: float = 0
    useful_life_years: int = 5
    conditions: List[str] = []
    compliance_status: str = "pending"


class GrantResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    grant_name: str
    granting_authority: str
    grant_amount: float
    grant_type: str
    recognition_method: str
    annual_recognition: float
    deferred_income_balance: float
    annual_amortization: float = 0
    journal_entries: List[str] = []
    disclosure_notes: List[str] = []
    conditions: List[str] = []
    compliance_status: str


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/recognize", response_model=GrantResult)
async def recognize_grant(req: GrantRequest):
    result = calc.recognize_grant(
        req.company_id,
        req.grant_name,
        req.granting_authority,
        req.grant_amount,
        req.grant_type.value,
        req.recognition_method.value,
        req.asset_useful_life,
        req.related_asset_cost,
        req.useful_life_years,
        req.conditions,
        req.compliance_status,
    )
    logger.info("Grant recognized", company=req.company_id, grant=req.grant_name)
    return GrantResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
