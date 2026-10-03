"""
Vimbai Data Warehouse Service
Manages dimensional data warehouse schemas, fact tables, and aggregate queries.

Records persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway).
Aggregate queries resolve the fact table among the caller's own
Book-visible facts. Response shapes preserved exactly.

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
if "data_warehouse_service" not in _sys.modules or not hasattr(_sys.modules.get("data_warehouse_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("data_warehouse_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["data_warehouse_service"] = _pkg
    _sys.modules["data_warehouse_service"].__path__ = [_HERE]

import structlog
from data_warehouse_service import crud
from data_warehouse_service.database import Neo4jConnector
from data_warehouse_service.dependencies import book_id_var, get_db_session, get_user_id
from data_warehouse_service.exceptions import DataWarehouseServiceError
from data_warehouse_service.models import AggregateQuery, DimensionTable, ETLJob, FactTable
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "data-warehouse-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8362"))

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

app = FastAPI(title="Vimbai Data Warehouse Service", version=SERVICE_VERSION, docs_url="/docs")
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


@app.exception_handler(DataWarehouseServiceError)
async def _dw_error(request: Request, exc: DataWarehouseServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/dimensions", response_model=DimensionTable)
async def create_dimension(
    name: str,
    columns: List[Dict[str, str]] = [],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a dimension table."""
    dim = DimensionTable(name=name, columns=columns)
    dim = await crud.create_dimension(db_session, user_id, dim)
    logger.info("Dimension created", dim_id=dim.id, name=name)
    return dim


@app.get("/dimensions", response_model=List[DimensionTable])
async def list_dimensions(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all dimension tables."""
    return await crud.list_dimensions(db_session, user_id)


@app.post("/facts", response_model=FactTable)
async def create_fact(
    name: str,
    dimensions: List[str] = [],
    measures: List[Dict[str, str]] = [],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a fact table."""
    fact = FactTable(name=name, dimensions=dimensions, measures=measures)
    fact = await crud.create_fact(db_session, user_id, fact)
    logger.info("Fact table created", fact_id=fact.id, name=name)
    return fact


@app.get("/facts", response_model=List[FactTable])
async def list_facts(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all fact tables."""
    return await crud.list_facts(db_session, user_id)


@app.post("/query", response_model=AggregateQuery)
async def run_aggregate_query(
    fact_table: str,
    group_by: List[str] = [],
    measures: List[str] = [],
    filters: Dict[str, Any] = {},
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Run an aggregate query against a fact table."""
    fact = await crud.get_fact_by_name(db_session, user_id, fact_table)
    if not fact:
        raise HTTPException(status_code=404, detail="Fact table not found")

    query = AggregateQuery(
        fact_table=fact_table,
        group_by=group_by,
        measures=measures,
        filters=filters,
        results=[],  # would return actual aggregated data
    )
    query = await crud.create_query(db_session, user_id, query)
    logger.info("Aggregate query executed", query_id=query.id, fact=fact_table)
    return query


@app.post("/etl", response_model=ETLJob)
async def create_etl_job(
    source: str,
    target: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create and run an ETL job."""
    job = ETLJob(
        source=source,
        target=target,
        status="running",
        started_at=datetime.now(timezone.utc),
    )
    # Simulate ETL completion
    job.status = "completed"
    job.completed_at = datetime.now(timezone.utc)
    job = await crud.create_etl_job(db_session, user_id, job)
    logger.info("ETL job completed", etl_id=job.id, source=source, target=target)
    return job


@app.get("/etl", response_model=List[ETLJob])
async def list_etl_jobs(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all ETL jobs."""
    return await crud.list_etl_jobs(db_session, user_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
