"""
Vimbai Goodwill Service
Goodwill impairment testing under IAS 36 with CGU allocation and recoverable amount.
Port: 8405
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "goodwill-service"
PORT = int(os.getenv("PORT", "8405"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Goodwill Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class CGU(BaseModel):
    cgu_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    carrying_value: float
    goodwill_allocated: float = 0
    fair_value: float = 0
    value_in_use: float = 0


class ImpairmentTestRequest(BaseModel):
    company_id: str
    fiscal_year: int
    cgu: CGU
    test_method: str = "higher_of"  # higher_of, fair_value_only, viu_only


class ImpairmentTestResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    cgu_name: str
    carrying_value: float
    recoverable_amount: float
    impairment_loss: float
    goodwill_impaired: float
    other_assets_impaired: float
    test_method: str
    is_impaired: bool
    post_impairment_carrying: float


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/test", response_model=ImpairmentTestResult)
async def test_impairment(req: ImpairmentTestRequest):
    if req.test_method == "fair_value_only":
        recoverable = req.cgu.fair_value
    elif req.test_method == "viu_only":
        recoverable = req.cgu.value_in_use
    else:
        recoverable = max(req.cgu.fair_value, req.cgu.value_in_use)

    impairment = max(req.cgu.carrying_value - recoverable, 0)
    goodwill_impaired = min(impairment, req.cgu.goodwill_allocated)
    other_impaired = impairment - goodwill_impaired
    post = req.cgu.carrying_value - impairment

    return ImpairmentTestResult(
        company_id=req.company_id,
        fiscal_year=req.fiscal_year,
        cgu_name=req.cgu.name,
        carrying_value=round(req.cgu.carrying_value, 2),
        recoverable_amount=round(recoverable, 2),
        impairment_loss=round(impairment, 2),
        goodwill_impaired=round(goodwill_impaired, 2),
        other_assets_impaired=round(other_impaired, 2),
        test_method=req.test_method,
        is_impaired=impairment > 0,
        post_impairment_carrying=round(post, 2),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
