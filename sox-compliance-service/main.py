"""
Vimbai SOX Compliance Service
Manages Sarbanes-Oxley (SOX) compliance controls, testing, and deficiency tracking.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "sox-compliance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8421"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai SOX Compliance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class Control(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    control_id_ref: str  # e.g. SOX-ITGC-001
    description: str
    control_type: str  # preventive, detective, corrective
    control_nature: str  # manual, automated, IT-dependent
    frequency: str  # daily, weekly, monthly, quarterly, annual
    owner: str
    process: str
    risk_level: str = "medium"  # low, medium, high
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ControlTest(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    control_id: str
    test_period: str
    tester: str
    sample_size: int = 25
    exceptions_found: int = 0
    result: str = "pass"  # pass, fail, pass_with_exception
    test_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    notes: str = ""


class Deficiency(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    control_id: str
    severity: str  # control_deficiency, significant_deficiency, material_weakness
    description: str
    remediation_plan: str = ""
    remediation_owner: str = ""
    status: str = "open"  # open, in_progress, remediated
    identified_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    remediated_date: Optional[datetime] = None


controls: List[Control] = []
tests: List[ControlTest] = []
deficiencies: List[Deficiency] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/controls", response_model=Control)
async def create_control(
    control_id_ref: str,
    description: str,
    control_type: str,
    control_nature: str,
    frequency: str,
    owner: str,
    process: str,
    risk_level: str = "medium",
):
    """Register a SOX control."""
    control = Control(
        control_id_ref=control_id_ref,
        description=description,
        control_type=control_type,
        control_nature=control_nature,
        frequency=frequency,
        owner=owner,
        process=process,
        risk_level=risk_level,
    )
    controls.append(control)
    logger.info("SOX control created", control_id=control.id, ref=control_id_ref)
    return control


@app.get("/controls", response_model=List[Control])
async def list_controls(process: Optional[str] = None, status: Optional[str] = None):
    """List SOX controls."""
    result = controls
    if process:
        result = [c for c in result if c.process == process]
    if status:
        result = [c for c in result if c.status == status]
    return result


@app.post("/controls/{control_id}/test", response_model=ControlTest)
async def test_control(
    control_id: str,
    test_period: str,
    tester: str,
    sample_size: int = 25,
    exceptions_found: int = 0,
    notes: str = "",
):
    """Record a control test result."""
    control = next((c for c in controls if c.id == control_id), None)
    if not control:
        raise HTTPException(status_code=404, detail="Control not found")

    result = (
        "pass" if exceptions_found == 0 else ("pass_with_exception" if exceptions_found < sample_size * 0.1 else "fail")
    )
    test = ControlTest(
        control_id=control_id,
        test_period=test_period,
        tester=tester,
        sample_size=sample_size,
        exceptions_found=exceptions_found,
        result=result,
        notes=notes,
    )
    tests.append(test)

    if result == "fail":
        deficiency = Deficiency(
            control_id=control_id,
            severity="significant_deficiency" if exceptions_found > sample_size * 0.2 else "control_deficiency",
            description=f"Control test failed with {exceptions_found} exceptions out of {sample_size} samples.",
            remediation_plan="TBD",
        )
        deficiencies.append(deficiency)

    logger.info("Control test recorded", control_id=control_id, result=result)
    return test


@app.get("/controls/{control_id}/tests", response_model=List[ControlTest])
async def list_tests(control_id: str):
    """List test results for a control."""
    return [t for t in tests if t.control_id == control_id]


@app.post("/deficiencies", response_model=Deficiency)
async def create_deficiency(
    control_id: str,
    severity: str,
    description: str,
    remediation_plan: str = "",
    remediation_owner: str = "",
):
    """Record a SOX deficiency."""
    valid_severities = ["control_deficiency", "significant_deficiency", "material_weakness"]
    if severity not in valid_severities:
        raise HTTPException(status_code=400, detail=f"Invalid severity. Must be one of {valid_severities}")

    deficiency = Deficiency(
        control_id=control_id,
        severity=severity,
        description=description,
        remediation_plan=remediation_plan,
        remediation_owner=remediation_owner,
    )
    deficiencies.append(deficiency)
    logger.info("Deficiency recorded", deficiency_id=deficiency.id, severity=severity)
    return deficiency


@app.get("/deficiencies", response_model=List[Deficiency])
async def list_deficiencies(status: Optional[str] = None):
    """List SOX deficiencies."""
    if status:
        return [d for d in deficiencies if d.status == status]
    return deficiencies


@app.put("/deficiencies/{deficiency_id}")
async def update_deficiency(deficiency_id: str, status: str, remediation_plan: str = ""):
    """Update a deficiency (e.g. mark as remediated)."""
    deficiency = next((d for d in deficiencies if d.id == deficiency_id), None)
    if not deficiency:
        raise HTTPException(status_code=404, detail="Deficiency not found")

    deficiency.status = status
    if remediation_plan:
        deficiency.remediation_plan = remediation_plan
    if status == "remediated":
        deficiency.remediated_date = datetime.now(timezone.utc)
    return deficiency


@app.get("/dashboard")
async def dashboard():
    """SOX compliance dashboard summary."""
    return {
        "total_controls": len(controls),
        "active_controls": len([c for c in controls if c.status == "active"]),
        "total_tests": len(tests),
        "passing_tests": len([t for t in tests if t.result == "pass"]),
        "failing_tests": len([t for t in tests if t.result == "fail"]),
        "open_deficiencies": len([d for d in deficiencies if d.status == "open"]),
        "material_weaknesses": len(
            [d for d in deficiencies if d.severity == "material_weakness" and d.status != "remediated"]
        ),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
