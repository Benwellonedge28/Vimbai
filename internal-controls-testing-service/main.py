"""
Vimbai Internal Controls Testing Service
SOX-style internal controls testing, deficiency classification, and remediation tracking.
Port: 8395
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
_spec = importlib.util.spec_from_file_location("internal_controls_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["internal_controls_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "internal-controls-testing-service"
PORT = int(os.getenv("PORT", "8395"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Internal Controls Testing Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class ControlType(str, Enum):
    PREVENTIVE = "preventive"
    DETECTIVE = "detective"
    CORRECTIVE = "corrective"


class DeficiencyLevel(str, Enum):
    NONE = "none"
    DEFICIENCY = "deficiency"
    SIGNIFICANT = "significant_deficiency"
    MATERIAL = "material_weakness"


class Control(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    control_type: ControlType
    description: str
    process: str
    test_sample_size: int = 25
    exceptions_found: int = 0
    last_tested: str = ""
    remediation_needed: bool = False


class TestingRequest(BaseModel):
    company_id: str
    fiscal_year: int
    controls: List[Control]


class TestingResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    total_controls: int
    controls_tested: int
    effective_controls: int
    deficient_controls: int
    material_weaknesses: int
    significant_deficiencies: int
    overall_assessment: str
    control_details: List[Dict] = []
    remediation_plan: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/test", response_model=TestingResult)
async def test_controls(req: TestingRequest):
    result = calc.test_controls(
        req.company_id,
        req.fiscal_year,
        [c.model_dump(mode="json") for c in req.controls],
    )
    logger.info("Controls tested", company=req.company_id, controls=result["total_controls"])
    return TestingResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
