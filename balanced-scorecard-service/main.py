"""
Vimbai Balanced Scorecard Service
Kaplan-Norton balanced scorecard with four perspectives and strategic alignment.
Port: 8390
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
_spec = importlib.util.spec_from_file_location("balanced_scorecard_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["balanced_scorecard_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "balanced-scorecard-service"
PORT = int(os.getenv("PORT", "8390"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Balanced Scorecard Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class KPI(BaseModel):
    name: str
    perspective: str  # financial, customer, internal, learning_growth
    target: float
    actual: float
    weight: float = 1.0
    unit: str = "number"


class ScorecardRequest(BaseModel):
    company_id: str
    period: str
    kpis: List[KPI]


class ScorecardResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    financial_score: float
    customer_score: float
    internal_score: float
    learning_score: float
    overall_score: float
    perspectives: Dict[str, Dict] = {}
    action_items: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/score", response_model=ScorecardResult)
async def calculate_scorecard(req: ScorecardRequest):
    perspectives = {
        "financial": {"kpis": [], "score": 0, "weight_total": 0},
        "customer": {"kpis": [], "score": 0, "weight_total": 0},
        "internal": {"kpis": [], "score": 0, "weight_total": 0},
        "learning_growth": {"kpis": [], "score": 0, "weight_total": 0},
    }

    action_items = []

    result = calc.scorecard(req.company_id, req.period, [k.model_dump() for k in req.kpis])
    return ScorecardResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
