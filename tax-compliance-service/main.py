"""Vimbai Tax Compliance Service - tax obligations, filings and compliance summaries. Port: 8374

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "tax_compliance_service" not in _sys.modules or not hasattr(_sys.modules.get("tax_compliance_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("tax_compliance_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["tax_compliance_service"] = _pkg
    _sys.modules["tax_compliance_service"].__path__ = [_HERE]

import os
from typing import List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from tax_compliance_service import crud, models
from tax_compliance_service.dependencies import book_id_var, get_db_session, get_user_id
from tax_compliance_service.exceptions import TaxComplianceError

SERVICE_NAME = "tax-compliance-service"
PORT = int(os.getenv("PORT", "8374"))
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
app = FastAPI(title="Vimbai Tax Compliance Service", version="2.0.0", docs_url="/docs")
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


@app.exception_handler(TaxComplianceError)
async def _tax_compliance_error(request: Request, exc: TaxComplianceError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/obligations", response_model=models.TaxObligation)
async def create_obligation(
    obligation: models.TaxObligationCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_obligation(db_session, user_id, obligation)
    logger.info("obligation_created", company_id=item.company_id, type=item.obligation_type)
    return item


@app.get("/obligations", response_model=List[models.TaxObligation])
async def list_obligations(
    company_id: str,
    status: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_obligations(db_session, user_id, company_id, status)


@app.post("/obligations/{obligation_id}/file")
async def file_obligation(
    obligation_id: str,
    company_id: str,
    filed_amount: float = 0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.file_obligation(db_session, user_id, company_id, obligation_id, filed_amount)
    except TaxComplianceError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/summary", response_model=models.ComplianceSummary)
async def get_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_summary(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
