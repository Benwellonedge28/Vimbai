"""
Vimbai ETL Service
Extract, Transform, Load pipeline for data integration across services.
"""

import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "etl-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8363"))

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

# Loaded bare (uvicorn main:app) and by path (root integration tests), so this
# directory has to be importable before the store module is pulled in.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import etl_store as store  # noqa: E402

app = FastAPI(title="Vimbai ETL Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class PipelineStep(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    step_type: str  # extract, transform, load
    source: str = ""
    target: str = ""
    config: Dict[str, Any] = {}
    status: str = "pending"
    rows_processed: int = 0
    error_message: str = ""
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None


class Pipeline(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    steps: List[PipelineStep] = []
    status: str = "pending"  # pending, running, completed, failed
    schedule: str = ""  # cron expression
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    last_run: Optional[datetime] = None


class CreatePipelineRequest(BaseModel):
    name: str
    description: str = ""
    steps: List[Dict[str, Any]]
    schedule: str = ""


# Pipelines are durable definitions: Neo4j, owned by the caller, gated by Book.
PIPELINES = ("Pipeline", "OWNS_PIPELINE")


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the caller and Book context (set by the gateway) to the store."""
    store.user_id_var.set(request.headers.get("X-User-Id"))
    store.book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


def _pipeline_model(node: Dict[str, Any]) -> Pipeline:
    steps = [PipelineStep(**step) for step in store.loads(node.get("steps_json"))]
    return Pipeline(
        id=node["id"],
        name=node.get("name", ""),
        description=node.get("description", ""),
        steps=steps,
        status=node.get("status", "pending"),
        schedule=node.get("schedule", ""),
        created_at=node.get("created_at"),
        last_run=node.get("last_run"),
    )


def _pipeline_props(pipeline: Pipeline) -> Dict[str, Any]:
    return {
        "name": pipeline.name,
        "description": pipeline.description,
        "steps_json": store.dumps([step.model_dump(mode="json") for step in pipeline.steps]),
        "status": pipeline.status,
        "schedule": pipeline.schedule,
        "created_at": pipeline.created_at.isoformat(),
        "last_run": pipeline.last_run.isoformat() if pipeline.last_run else None,
    }


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/pipelines", response_model=Pipeline)
async def create_pipeline(request: CreatePipelineRequest):
    """Create an ETL pipeline."""
    steps = [PipelineStep(**s) for s in request.steps]
    pipeline = Pipeline(
        name=request.name,
        description=request.description,
        steps=steps,
        schedule=request.schedule,
    )
    node = await store.create_node(PIPELINES[0], PIPELINES[1], {"id": pipeline.id, **_pipeline_props(pipeline)})
    logger.info("ETL pipeline created", pipeline_id=pipeline.id, name=request.name, steps=len(steps))
    return _pipeline_model(node)


@app.get("/pipelines", response_model=List[Pipeline])
async def list_pipelines(status: Optional[str] = None):
    """List ETL pipelines."""
    nodes = await store.list_nodes(PIPELINES[0], PIPELINES[1], {"status": status})
    return [_pipeline_model(node) for node in nodes]


@app.get("/pipelines/{pipeline_id}", response_model=Pipeline)
async def get_pipeline(pipeline_id: str):
    """Get a specific pipeline."""
    node = await store.get_node(PIPELINES[0], PIPELINES[1], pipeline_id)
    if not node:
        raise HTTPException(status_code=404, detail="Pipeline not found")
    return _pipeline_model(node)


@app.post("/pipelines/{pipeline_id}/run")
async def run_pipeline(pipeline_id: str):
    """Execute an ETL pipeline."""
    node = await store.get_node(PIPELINES[0], PIPELINES[1], pipeline_id)
    if not node:
        raise HTTPException(status_code=404, detail="Pipeline not found")

    pipeline = _pipeline_model(node)
    pipeline.status = "running"
    pipeline.last_run = datetime.now(timezone.utc)
    logger.info("ETL pipeline started", pipeline_id=pipeline_id)

    finished = datetime.now(timezone.utc).isoformat()
    for step in pipeline.steps:
        step.status = "running"
        step.started_at = datetime.now(timezone.utc)
        # Simulate ETL step
        step.status = "completed"
        step.rows_processed = 0  # would be actual count
        step.completed_at = datetime.now(timezone.utc)
        logger.info("ETL step completed", step=step.name, type=step.step_type)

    pipeline.status = "completed"
    await store.update_node(
        PIPELINES[0],
        PIPELINES[1],
        pipeline_id,
        {
            "status": pipeline.status,
            "last_run": pipeline.last_run.isoformat() if pipeline.last_run else finished,
            "steps_json": store.dumps([step.model_dump(mode="json") for step in pipeline.steps]),
        },
    )
    return {
        "pipeline_id": pipeline_id,
        "status": "completed",
        "steps_completed": len(pipeline.steps),
        "run_at": pipeline.last_run.isoformat(),
    }


@app.delete("/pipelines/{pipeline_id}")
async def delete_pipeline(pipeline_id: str):
    """Delete an ETL pipeline."""
    node = await store.get_node(PIPELINES[0], PIPELINES[1], pipeline_id)
    if not node:
        raise HTTPException(status_code=404, detail="Pipeline not found")

    await store.delete_node(PIPELINES[0], PIPELINES[1], pipeline_id)
    return {"deleted": True, "pipeline_id": pipeline_id}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
