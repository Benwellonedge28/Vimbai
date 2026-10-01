"""Vimbai Insurance Claims Service - claim filing and settlement processing. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "insurance_claims_service" not in _sys.modules or not hasattr(
    _sys.modules.get("insurance_claims_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("insurance_claims_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["insurance_claims_service"] = _pkg
    _sys.modules["insurance_claims_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from insurance_claims_service import crud, models
from insurance_claims_service.dependencies import book_id_var, get_db_session, get_user_id
from insurance_claims_service.exceptions import InsuranceClaimsError
from neo4j import AsyncSession

SERVICE_NAME = "insurance-claims-service"
PORT = int(os.getenv("PORT", "8370"))
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
app = FastAPI(title="Vimbai Insurance Claims Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(InsuranceClaimsError)
async def _insurance_claims_error(request: Request, exc: InsuranceClaimsError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/file", response_model=models.InsuranceClaim)
async def file_claim(
    claim: models.InsuranceClaimCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.file_claim(db_session, user_id, claim)
    logger.info("Claim filed", claim_id=item.id, company=item.company_id)
    return item


@app.get("/claims", response_model=models.List[models.InsuranceClaim])
async def list_claims(
    company_id: str,
    status: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_claims(db_session, user_id, company_id, status)


@app.post("/claims/{claim_id}/process", response_model=models.ClaimResult)
async def process_claim(
    claim_id: str,
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.process_claim(db_session, user_id, company_id, claim_id)
    except InsuranceClaimsError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
