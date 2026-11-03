"""
Vimbai Tax Risk Service
Tax risk assessment, exposure quantification, and mitigation recommendations.
Port: 8373
"""

import importlib.util
import os
import sys
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("tax_risk_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["tax_risk_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "tax-risk-service"
PORT = int(os.getenv("PORT", "8373"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Tax Risk Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class RiskLevel(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class TaxRiskItem(BaseModel):
    risk_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    description: str
    risk_type: str  # transfer_pricing, vat_gap, permanent_establishment, treaty, reporting
    potential_exposure: float
    probability: float = 0.5
    jurisdiction: str = "ZW"
    mitigation_actions: List[str] = []


class TaxRiskRequest(BaseModel):
    company_id: str
    risks: List[TaxRiskItem]


class TaxRiskAssessment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    assessment_date: str
    overall_risk_level: RiskLevel
    total_exposure: float
    weighted_exposure: float
    risk_count: int
    risks: List[Dict]
    recommendations: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/assess", response_model=TaxRiskAssessment)
async def assess_risk(req: TaxRiskRequest):
    result = calc.assess_risk(
        req.company_id,
        datetime.now(timezone.utc).isoformat(),
        [r.model_dump() for r in req.risks],
    )
    logger.info("Tax risk assessed", company=req.company_id, level=result["overall_risk_level"])
    return TaxRiskAssessment(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
