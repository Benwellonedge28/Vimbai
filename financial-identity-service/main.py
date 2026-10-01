"""Vimbai Financial Identity Service - Financial identity verification. Port: 8372

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "financial_identity_service" not in _sys.modules or not hasattr(
    _sys.modules.get("financial_identity_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("financial_identity_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["financial_identity_service"] = _pkg
    _sys.modules["financial_identity_service"].__path__ = [_HERE]

import os
from typing import List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from financial_identity_service import crud, models
from financial_identity_service.dependencies import book_id_var, get_db_session, get_user_id
from financial_identity_service.exceptions import FinancialIdentityError
from neo4j import AsyncSession

SERVICE_NAME = "financial-identity-service"
PORT = int(os.getenv("PORT", "8372"))
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
app = FastAPI(title="Vimbai Financial Identity Service", version="2.0.0", docs_url="/docs")
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


@app.exception_handler(FinancialIdentityError)
async def _identity_error(request: Request, exc: FinancialIdentityError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/profiles", response_model=models.FinancialProfile)
async def create_profile(
    profile: models.FinancialProfileCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    created = await crud.create_profile(db_session, user_id, profile)
    logger.info("kyc_profile_created", subject=created.user_id)
    return created


@app.get("/profiles/{profile_id}", response_model=models.FinancialProfile)
async def get_profile(
    profile_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.get_profile(db_session, user_id, profile_id)
    except FinancialIdentityError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.put("/profiles/{profile_id}/verify")
async def verify_profile(
    profile_id: str,
    documents: List[str],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.verify_profile(db_session, user_id, profile_id, documents)
    except FinancialIdentityError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/profiles/user/{user_id}", response_model=models.FinancialProfile)
async def get_by_user(
    user_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.get_by_user(db_session, caller_id, user_id)
    except FinancialIdentityError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
