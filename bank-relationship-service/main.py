"""Vimbai Bank Relationship Service - bank relationships and service quality. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "bank_relationship_service" not in _sys.modules or not hasattr(
    _sys.modules.get("bank_relationship_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("bank_relationship_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["bank_relationship_service"] = _pkg
    _sys.modules["bank_relationship_service"].__path__ = [_HERE]

import os

import structlog
from bank_relationship_service import crud, models
from bank_relationship_service.dependencies import book_id_var, get_db_session, get_user_id
from bank_relationship_service.exceptions import BankRelationshipError
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "bank-relationship-service"
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
app = FastAPI(title="Vimbai Bank Relationship Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(BankRelationshipError)
async def _bank_relationship_error(request: Request, exc: BankRelationshipError):
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


@app.post("/relationships", response_model=models.BankRelationship)
async def create_relationship(
    rel: models.BankRelationshipCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_relationship(db_session, user_id, rel)
    logger.info("relationship_created", company_id=item.company_id, bank=item.bank_name)
    return item


@app.get("/relationships/{company_id}")
async def get_relationships(
    company_id: str,
    status_filter: str = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    rels = await crud.list_relationships(db_session, user_id, company_id, status_filter or "")
    return {"company_id": company_id, "relationships": rels, "total": len(rels)}


@app.put("/relationships/{rel_id}")
async def update_relationship(
    rel_id: str,
    rating: int = None,
    status: models.RelationshipStatus = None,
    notes: str = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.update_relationship(db_session, user_id, rel_id, rating, status, notes)
    except BankRelationshipError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.post("/quality-metrics")
async def add_quality_metric(
    metric: models.ServiceQualityMetricCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.add_quality_metric(db_session, user_id, metric)
    except BankRelationshipError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/quality-metrics/{relationship_id}")
async def get_quality_metrics(
    relationship_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        return await crud.get_quality_metrics(db_session, user_id, relationship_id)
    except BankRelationshipError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.get("/summary/{company_id}")
async def relationship_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.relationship_summary(db_session, user_id, company_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
