"""
Vimbai Data Warehouse Service
Manages dimensional data warehouse schemas, fact tables, and aggregate queries.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "data-warehouse-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8417"))

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


class DimensionTable(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    columns: List[Dict[str, str]] = []  # [{"name": "id", "type": "int"}, ...]
    row_count: int = 0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class FactTable(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    dimensions: List[str] = []  # dimension table names
    measures: List[Dict[str, str]] = []  # [{"name": "amount", "type": "decimal", "agg": "sum"}]
    row_count: int = 0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AggregateQuery(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    fact_table: str
    group_by: List[str] = []
    measures: List[str] = []
    filters: Dict[str, Any] = {}
    results: List[Dict[str, Any]] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ETLJob(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    source: str
    target: str
    status: str = "pending"
    rows_processed: int = 0
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None


dimensions: List[DimensionTable] = []
facts: List[FactTable] = []
queries: List[AggregateQuery] = []
etl_jobs: List[ETLJob] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/dimensions", response_model=DimensionTable)
async def create_dimension(name: str, columns: List[Dict[str, str]] = []):
    """Create a dimension table."""
    dim = DimensionTable(name=name, columns=columns)
    dimensions.append(dim)
    logger.info("Dimension created", dim_id=dim.id, name=name)
    return dim


@app.get("/dimensions", response_model=List[DimensionTable])
async def list_dimensions():
    """List all dimension tables."""
    return dimensions


@app.post("/facts", response_model=FactTable)
async def create_fact(name: str, dimensions: List[str] = [], measures: List[Dict[str, str]] = []):
    """Create a fact table."""
    fact = FactTable(name=name, dimensions=dimensions, measures=measures)
    facts.append(fact)
    logger.info("Fact table created", fact_id=fact.id, name=name)
    return fact


@app.get("/facts", response_model=List[FactTable])
async def list_facts():
    """List all fact tables."""
    return facts


@app.post("/query", response_model=AggregateQuery)
async def run_aggregate_query(
    fact_table: str,
    group_by: List[str] = [],
    measures: List[str] = [],
    filters: Dict[str, Any] = {},
):
    """Run an aggregate query against a fact table."""
    fact = next((f for f in facts if f.name == fact_table), None)
    if not fact:
        raise HTTPException(status_code=404, detail="Fact table not found")

    query = AggregateQuery(
        fact_table=fact_table,
        group_by=group_by,
        measures=measures,
        filters=filters,
        results=[],  # would return actual aggregated data
    )
    queries.append(query)
    logger.info("Aggregate query executed", query_id=query.id, fact=fact_table)
    return query


@app.post("/etl", response_model=ETLJob)
async def create_etl_job(source: str, target: str):
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
    etl_jobs.append(job)
    logger.info("ETL job completed", etl_id=job.id, source=source, target=target)
    return job


@app.get("/etl", response_model=List[ETLJob])
async def list_etl_jobs():
    """List all ETL jobs."""
    return etl_jobs


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
