"""
Vimbai Amortization Service
Intangible asset amortization with straight-line and reducing balance methods.
Port: 8357
"""

import importlib.util
import math
import os
import uuid
from datetime import date, datetime, timezone
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "amortization-service"
PORT = int(os.getenv("PORT", "8357"))
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
_spec = importlib.util.spec_from_file_location("amortization_service_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(calc)

logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Amortization Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class IntangibleAsset(BaseModel):
    intangible_id: str
    type: str  # patent, trademark, copyright, goodwill, software, license
    cost: float
    accumulated_amortization: float = 0
    useful_life_years: int
    residual_value: float = 0
    method: str = "straight_line"  # straight_line, reducing_balance
    rate: float = 0.25  # for reducing balance


class AmortizationRequest(BaseModel):
    company_id: str
    intangibles: List[IntangibleAsset]
    period_start: date
    period_end: date


class AmortizationSchedule(BaseModel):
    intangible_id: str
    type: str
    cost: float
    residual_value: float
    annual_amortization: float
    monthly_amortization: float
    accumulated_amortization: float
    net_book_value: float
    method: str
    remaining_life_years: float


class AmortizationResponse(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: Dict[str, str]
    total_amortization: float
    total_net_book_value: float
    schedules: List[AmortizationSchedule] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/calculate", response_model=AmortizationResponse)
async def calculate_amortization(req: AmortizationRequest):
    logger.info("Calculating amortization", company=req.company_id, count=len(req.intangibles))
    result = calc.calculate(
        req.company_id,
        [a.model_dump() for a in req.intangibles],
        req.period_start,
        req.period_end,
        _reference,
    )
    return AmortizationResponse(**result)


def _reference(intangibles, period_start, period_end):
    """The original Python calculation, kept as the oracle."""
    total_amort = 0
    total_nbv = 0
    schedules = []

    for asset in intangibles:
        depreciable_amount = asset["cost"] - asset["residual_value"]

        if asset["method"] == "straight_line":
            annual = depreciable_amount / asset["useful_life_years"] if asset["useful_life_years"] > 0 else 0
        else:  # reducing_balance
            annual = (asset["cost"] - asset["accumulated_amortization"]) * asset["rate"]

        monthly = annual / 12
        new_accumulated = asset["accumulated_amortization"] + annual
        nbv = asset["cost"] - new_accumulated
        remaining_years = nbv / annual if annual > 0 else 0

        total_amort += annual
        total_nbv += nbv

        schedules.append(
            {
                "intangible_id": asset["intangible_id"],
                "type": asset["type"],
                "cost": asset["cost"],
                "residual_value": asset["residual_value"],
                "annual_amortization": round(annual, 2),
                "monthly_amortization": round(monthly, 2),
                "accumulated_amortization": round(new_accumulated, 2),
                "net_book_value": round(nbv, 2),
                "method": asset["method"],
                "remaining_life_years": round(remaining_years, 1),
            }
        )

    return {
        "period": {"start": str(period_start), "end": str(period_end)},
        "total_amortization": round(total_amort, 2),
        "total_net_book_value": round(total_nbv, 2),
        "schedules": schedules,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
