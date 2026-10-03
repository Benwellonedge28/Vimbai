"""
Vimbai SOX Compliance Service
Manages Sarbanes-Oxley (SOX) compliance controls, testing, and deficiency tracking.

Records persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway). Control
tests and deficiency updates check the caller's own Book-visible
records first. Result thresholds and response shapes preserved exactly.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "sox_compliance_service" not in _sys.modules or not hasattr(_sys.modules.get("sox_compliance_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("sox_compliance_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["sox_compliance_service"] = _pkg
    _sys.modules["sox_compliance_service"].__path__ = [_HERE]

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from sox_compliance_service import crud
from sox_compliance_service.database import Neo4jConnector
from sox_compliance_service.dependencies import book_id_var, get_db_session, get_user_id
from sox_compliance_service.exceptions import SoxComplianceServiceError
from sox_compliance_service.models import Control, ControlTest, Deficiency

SERVICE_NAME = "sox-compliance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8286"))

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(SoxComplianceServiceError)
async def _sox_error(request: Request, exc: SoxComplianceServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
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
    control = await crud.create_control(db_session, user_id, control)
    logger.info("SOX control created", control_id=control.id, ref=control_id_ref)
    return control


@app.get("/controls", response_model=List[Control])
async def list_controls(
    process: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List SOX controls."""
    result = await crud.list_controls(db_session, user_id)
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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record a control test result."""
    control = await crud.get_control(db_session, user_id, control_id)
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
    test = await crud.create_test(db_session, user_id, test)

    if result == "fail":
        deficiency = Deficiency(
            control_id=control_id,
            severity="significant_deficiency" if exceptions_found > sample_size * 0.2 else "control_deficiency",
            description=f"Control test failed with {exceptions_found} exceptions out of {sample_size} samples.",
            remediation_plan="TBD",
        )
        await crud.create_deficiency(db_session, user_id, deficiency)

    logger.info("Control test recorded", control_id=control_id, result=result)
    return test


@app.get("/controls/{control_id}/tests", response_model=List[ControlTest])
async def list_tests(
    control_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List test results for a control."""
    return await crud.list_tests(db_session, user_id, control_id)


@app.post("/deficiencies", response_model=Deficiency)
async def create_deficiency(
    control_id: str,
    severity: str,
    description: str,
    remediation_plan: str = "",
    remediation_owner: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
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
    deficiency = await crud.create_deficiency(db_session, user_id, deficiency)
    logger.info("Deficiency recorded", deficiency_id=deficiency.id, severity=severity)
    return deficiency


@app.get("/deficiencies", response_model=List[Deficiency])
async def list_deficiencies(
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List SOX deficiencies."""
    result = await crud.list_deficiencies(db_session, user_id)
    if status:
        return [d for d in result if d.status == status]
    return result


@app.put("/deficiencies/{deficiency_id}")
async def update_deficiency(
    deficiency_id: str,
    status: str,
    remediation_plan: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a deficiency (e.g. mark as remediated)."""
    deficiency = await crud.get_deficiency(db_session, user_id, deficiency_id)
    if not deficiency:
        raise HTTPException(status_code=404, detail="Deficiency not found")

    deficiency.status = status
    if remediation_plan:
        deficiency.remediation_plan = remediation_plan
    if status == "remediated":
        deficiency.remediated_date = datetime.now(timezone.utc)
    await crud.save_deficiency(db_session, user_id, deficiency)
    return deficiency


@app.get("/dashboard")
async def dashboard(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """SOX compliance dashboard summary."""
    all_controls = await crud.list_controls(db_session, user_id)
    all_tests = await crud.list_all_tests(db_session, user_id)
    all_deficiencies = await crud.list_deficiencies(db_session, user_id)
    return {
        "total_controls": len(all_controls),
        "active_controls": len([c for c in all_controls if c.status == "active"]),
        "total_tests": len(all_tests),
        "passing_tests": len([t for t in all_tests if t.result == "pass"]),
        "failing_tests": len([t for t in all_tests if t.result == "fail"]),
        "open_deficiencies": len([d for d in all_deficiencies if d.status == "open"]),
        "material_weaknesses": len(
            [d for d in all_deficiencies if d.severity == "material_weakness" and d.status != "remediated"]
        ),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
