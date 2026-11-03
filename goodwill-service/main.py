"""
Vimbai Goodwill Service
Goodwill impairment testing under IAS 36 with CGU allocation and recoverable amount.
Port: 8405
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
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("goodwill_service_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["goodwill_service_calc"] = calc
_spec.loader.exec_module(calc)

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
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    if payload["test_method"] == "fair_value_only":
        recoverable = payload["cgu"]["fair_value"]
    elif payload["test_method"] == "viu_only":
        recoverable = payload["cgu"]["value_in_use"]
    else:
        recoverable = max(payload["cgu"]["fair_value"], payload["cgu"]["value_in_use"])

    impairment = max(payload["cgu"]["carrying_value"] - recoverable, 0)
    goodwill_impaired = min(impairment, payload["cgu"]["goodwill_allocated"])
    other_impaired = impairment - goodwill_impaired
    post = payload["cgu"]["carrying_value"] - impairment

    return dict(
        company_id=payload["company_id"],
        fiscal_year=payload["fiscal_year"],
        cgu_name=payload["cgu"]["name"],
        carrying_value=round(payload["cgu"]["carrying_value"], 2),
        recoverable_amount=round(recoverable, 2),
        impairment_loss=round(impairment, 2),
        goodwill_impaired=round(goodwill_impaired, 2),
        other_assets_impaired=round(other_impaired, 2),
        test_method=payload["test_method"],
        is_impaired=impairment > 0,
        post_impairment_carrying=round(post, 2),
    )


@app.post("/test", response_model=ImpairmentTestResult)
async def test_impairment(request: ImpairmentTestRequest):
    logger.info("Calculating", company=request.company_id)
    result = calc.run(request.model_dump(mode="json"), _reference)
    result.setdefault("id", str(uuid.uuid4()))
    return ImpairmentTestResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
