"""
Vimbai Divestiture Service
Disposal accounting, gain/loss calculation, and retained interest measurement.
Port: 8387
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

SERVICE_NAME = "divestiture-service"
PORT = int(os.getenv("PORT", "8387"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("divestiture_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["divestiture_calc"] = calc
_spec.loader.exec_module(calc)

logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Divestiture Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class DivestitureRequest(BaseModel):
    company_id: str
    subsidiary_name: str
    carrying_value: float
    disposal_proceeds: float
    disposal_costs: float = 0
    retained_interest_fair_value: float = 0
    retained_interest_pct: float = 0
    associated_goodwill: float = 0
    disposal_method: str = "sale"  # sale, spin_off, liquidation


class DivestitureResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    subsidiary_name: str
    disposal_method: str
    carrying_value: float
    net_proceeds: float
    gain_or_loss: float
    retained_interest_fv: float
    total_gain_recognized: float
    goodwill_derecognized: float
    journal_entries: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


class _Payload:
    """The request as the original code saw it: attributes, not keys."""

    def __init__(self, payload):
        self.__dict__.update(payload)


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    req = _Payload(payload)
    net_proceeds = req.disposal_proceeds - req.disposal_costs
    gain_loss = net_proceeds - (req.carrying_value - req.associated_goodwill)

    if req.retained_interest_pct > 0 and req.retained_interest_fair_value > 0:
        total_gain = gain_loss + req.retained_interest_fair_value
        entries = [
            f"Dr Cash/Bank ({round(net_proceeds, 2)})",
            f"Dr Investment in Subsidiary (retained) ({round(req.retained_interest_fair_value, 2)})",
            f"Cr Investment in Subsidiary ({round(req.carrying_value, 2)})",
            (
                f"Cr Gain on Disposal ({round(total_gain, 2)})"
                if total_gain > 0
                else f"Dr Loss on Disposal ({round(abs(total_gain), 2)})"
            ),
            "Cr Goodwill derecognized" if req.associated_goodwill > 0 else "",
        ]
    else:
        total_gain = gain_loss
        entries = [
            f"Dr Cash/Bank ({round(net_proceeds, 2)})",
            f"Cr Investment in Subsidiary ({round(req.carrying_value, 2)})",
            (
                f"Cr Gain on Disposal ({round(total_gain, 2)})"
                if total_gain > 0
                else f"Dr Loss on Disposal ({round(abs(total_gain), 2)})"
            ),
            "Dr/Cr Goodwill adjustment" if req.associated_goodwill > 0 else "",
        ]
    entries = [e for e in entries if e]

    return dict(
        company_id=req.company_id,
        subsidiary_name=req.subsidiary_name,
        disposal_method=req.disposal_method,
        carrying_value=round(req.carrying_value, 2),
        net_proceeds=round(net_proceeds, 2),
        gain_or_loss=round(gain_loss, 2),
        retained_interest_fv=round(req.retained_interest_fair_value, 2),
        total_gain_recognized=round(total_gain, 2),
        goodwill_derecognized=round(req.associated_goodwill, 2),
        journal_entries=entries,
    )


@app.post("/dispose", response_model=DivestitureResult)
async def calculate_disposal(request: DivestitureRequest):
    logger.info("Calculating a disposal", subsidiary=request.subsidiary_name)
    result = calc.dispose(request.model_dump(), _reference)
    result.setdefault("id", str(uuid.uuid4()))
    return DivestitureResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
