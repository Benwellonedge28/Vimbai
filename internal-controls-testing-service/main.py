"""
Vimbai Internal Controls Testing Service
SOX-style internal controls testing, deficiency classification, and remediation tracking.
Port: 8395
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
from enum import Enum
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "internal-controls-testing-service"
PORT = int(os.getenv("PORT", "8395"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Internal Controls Testing Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class ControlType(str, Enum):
    PREVENTIVE = "preventive"; DETECTIVE = "detective"; CORRECTIVE = "corrective"

class DeficiencyLevel(str, Enum):
    NONE = "none"; DEFICIENCY = "deficiency"; SIGNIFICANT = "significant_deficiency"; MATERIAL = "material_weakness"

class Control(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str; control_type: ControlType
    description: str; process: str
    test_sample_size: int = 25; exceptions_found: int = 0
    last_tested: str = ""; remediation_needed: bool = False

class TestingRequest(BaseModel):
    company_id: str; fiscal_year: int; controls: List[Control]

class TestingResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; fiscal_year: int
    total_controls: int; controls_tested: int
    effective_controls: int; deficient_controls: int
    material_weaknesses: int; significant_deficiencies: int
    overall_assessment: str
    control_details: List[Dict] = []
    remediation_plan: List[Dict] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/test", response_model=TestingResult)
async def test_controls(req: TestingRequest):
    effective = 0; deficient = 0; material = 0; significant = 0
    details = []; remediation = []
    
    for ctrl in req.controls:
        exception_rate = ctrl.exceptions_found / ctrl.test_sample_size if ctrl.test_sample_size else 0
        
        if exception_rate == 0:
            level = DeficiencyLevel.NONE; effective += 1
        elif exception_rate <= 0.05:
            level = DeficiencyLevel.DEFICIENCY; deficient += 1
        elif exception_rate <= 0.10:
            level = DeficiencyLevel.SIGNIFICANT; significant += 1; deficient += 1
        else:
            level = DeficiencyLevel.MATERIAL; material += 1; deficient += 1
        
        if level != DeficiencyLevel.NONE:
            remediation.append({
                "control_id": ctrl.id, "control_name": ctrl.name,
                "deficiency_level": level.value,
                "exception_rate": round(exception_rate * 100, 1),
                "recommended_action": f"Redesign {ctrl.name} - increase sample testing frequency"
            })
        
        details.append({
            "id": ctrl.id, "name": ctrl.name, "type": ctrl.control_type.value,
            "process": ctrl.process, "sample_size": ctrl.test_sample_size,
            "exceptions": ctrl.exceptions_found,
            "exception_rate": round(exception_rate * 100, 1),
            "deficiency_level": level.value, "effective": level == DeficiencyLevel.NONE
        })
    
    total = len(req.controls)
    if material > 0:
        assessment = "Adverse opinion - material weaknesses identified"
    elif significant > 0:
        assessment = "Qualified opinion - significant deficiencies identified"
    else:
        assessment = "Unqualified opinion - controls operating effectively"
    
    return TestingResult(
        company_id=req.company_id, fiscal_year=req.fiscal_year,
        total_controls=total, controls_tested=total,
        effective_controls=effective, deficient_controls=deficient,
        material_weaknesses=material, significant_deficiencies=significant,
        overall_assessment=assessment, control_details=details,
        remediation_plan=remediation
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
