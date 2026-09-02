"""
Vimbai Government Grants Service
IAS 20 government grant recognition, deferred income tracking, and compliance.
Port: 8408
"""

import os
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

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
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/recognize", response_model=GrantResult)
async def recognize_grant(req: GrantRequest):
    if req.grant_type == GrantType.ASSET:
        annual = req.grant_amount / req.asset_useful_life if req.asset_useful_life else 0
        deferred = req.grant_amount - annual  # year 1 recognition
    else:
        annual = req.grant_amount  # income grant recognized immediately
        deferred = 0

    return GrantResult(
        company_id=req.company_id,
        grant_name=req.grant_name,
        granting_authority=req.granting_authority,
        grant_amount=round(req.grant_amount, 2),
        grant_type=req.grant_type.value,
        recognition_method=req.recognition_method.value,
        annual_recognition=round(annual, 2),
        annual_amortization=round(annual, 2),
        deferred_income_balance=round(deferred, 2),
        journal_entries=[
            f"Dr Bank {round(req.grant_amount, 2)}",
            f"Cr Deferred Income {round(req.grant_amount, 2)}",
        ],
        disclosure_notes=[
            f"Grant received from {req.granting_authority or 'government authority'}",
            f"Recognized over {req.asset_useful_life} years",
            f"Conditions: {', '.join(req.conditions) if req.conditions else 'None specified'}",
        ],
        conditions=req.conditions,
        compliance_status=req.compliance_status,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
