"""Vimbai Appropriation Control Service - departmental budget controls. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "appropriation_control_service" not in _sys.modules or not hasattr(
    _sys.modules.get("appropriation_control_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("appropriation_control_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["appropriation_control_service"] = _pkg
    _sys.modules["appropriation_control_service"].__path__ = [_HERE]

import os

import structlog
from appropriation_control_service import crud, models
from appropriation_control_service.dependencies import book_id_var, get_db_session, get_user_id
from appropriation_control_service.exceptions import AppropriationControlError
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "appropriation-control-service"
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
app = FastAPI(title="Vimbai Appropriation Control Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(AppropriationControlError)
async def _appropriation_error(request: Request, exc: AppropriationControlError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/appropriations", response_model=models.Appropriation)
async def create_appropriation(
    appr: models.AppropriationCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_appropriation(db_session, user_id, appr)
    logger.info("appropriation_created", company_id=item.company_id, department=item.department)
    return item


@app.get("/appropriations/{company_id}")
async def get_appropriations(
    company_id: str,
    department: str = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    apprs = await crud.list_appropriations(db_session, user_id, company_id, department or "")
    return {"company_id": company_id, "appropriations": apprs, "total": len(apprs)}


@app.post("/transactions")
async def create_transaction(
    tx: models.AppropriationTransactionCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.create_transaction(db_session, user_id, tx)
    except AppropriationControlError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/check/{appropriation_id}")
async def check_available(
    appropriation_id: str,
    amount: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.check_available(db_session, user_id, appropriation_id, amount)
    except AppropriationControlError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
