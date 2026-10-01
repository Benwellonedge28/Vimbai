"""Vimbai Regulatory Compliance Service - regulation tracking and compliance dashboards. Port: 8378

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "regulatory_compliance_service" not in _sys.modules or not hasattr(
    _sys.modules.get("regulatory_compliance_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("regulatory_compliance_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["regulatory_compliance_service"] = _pkg
    _sys.modules["regulatory_compliance_service"].__path__ = [_HERE]

import os
from typing import List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from regulatory_compliance_service import crud, models
from regulatory_compliance_service.dependencies import book_id_var, get_db_session, get_user_id
from regulatory_compliance_service.exceptions import RegulatoryComplianceError

SERVICE_NAME = "regulatory-compliance-service"
PORT = int(os.getenv("PORT", "8378"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Regulatory Compliance Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(RegulatoryComplianceError)
async def _regulatory_compliance_error(request: Request, exc: RegulatoryComplianceError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/regulations", response_model=models.Regulation)
async def add_regulation(
    reg: models.RegulationCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_regulation(db_session, user_id, reg)
    logger.info("regulation_added", company_id=item.company_id, framework=item.framework)
    return item


@app.get("/regulations", response_model=List[models.Regulation])
async def list_regulations(
    company_id: str,
    framework: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_regulations(db_session, user_id, company_id, framework)


@app.post("/regulations/{reg_id}/update")
async def update_reg_status(
    reg_id: str,
    company_id: str,
    status: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.update_reg_status(db_session, user_id, company_id, reg_id, status)
    except RegulatoryComplianceError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/dashboard", response_model=models.ComplianceDashboard)
async def get_dashboard(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_dashboard(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
