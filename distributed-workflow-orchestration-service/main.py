"""
Vimbai Distributed Workflow Orchestration Service
Orchestrates multi-step workflows across distributed services with retry and compensation.
"""

import os
import sys
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "distributed-workflow-orchestration-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8046"))

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

import workflow_store as store  # noqa: E402

app = FastAPI(title="Vimbai Distributed Workflow Orchestration", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class WorkflowStatus(str, Enum):
    pending = "pending"
    running = "running"
    completed = "completed"
    failed = "failed"
    compensated = "compensated"
    cancelled = "cancelled"


class StepStatus(str, Enum):
    pending = "pending"
    running = "running"
    completed = "completed"
    failed = "failed"
    skipped = "skipped"
    compensated = "compensated"


class WorkflowStep(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    service_url: str
    endpoint: str
    method: str = "POST"
    payload: Dict[str, Any] = {}
    compensation_url: str = ""
    max_retries: int = 3
    retry_count: int = 0
    status: StepStatus = StepStatus.pending
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    error_message: str = ""


class Workflow(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    steps: List[WorkflowStep] = []
    status: WorkflowStatus = WorkflowStatus.pending
    current_step_index: int = 0
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    created_by: str = ""


class CreateWorkflowRequest(BaseModel):
    name: str
    description: str = ""
    steps: List[Dict[str, Any]]
    created_by: str = ""


# Workflows are durable records: Neo4j, owned by the caller, gated by Book.
WORKFLOWS = ("Workflow", "OWNS_WORKFLOW")


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the caller and Book context (set by the gateway) to the store."""
    store.user_id_var.set(request.headers.get("X-User-Id"))
    store.book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


def _workflow_model(node: Dict[str, Any]) -> Workflow:
    steps = [WorkflowStep(**step) for step in store.loads(node.get("steps_json"))]
    return Workflow(
        id=node["id"],
        name=node.get("name", ""),
        description=node.get("description", ""),
        steps=steps,
        status=WorkflowStatus(node.get("status", "pending")),
        current_step_index=int(node.get("current_step_index") or 0),
        created_at=node.get("created_at"),
        updated_at=node.get("updated_at"),
        created_by=node.get("created_by", ""),
    )


def _steps_json(workflow: Workflow) -> str:
    return store.dumps([step.model_dump(mode="json") for step in workflow.steps])


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/workflows", response_model=Workflow)
async def create_workflow(request: CreateWorkflowRequest):
    """Create a new distributed workflow."""
    steps = [WorkflowStep(**s) for s in request.steps]
    workflow = Workflow(
        name=request.name,
        description=request.description,
        steps=steps,
        created_by=request.created_by,
    )
    node = await store.create_node(
        WORKFLOWS[0],
        WORKFLOWS[1],
        {
            "id": workflow.id,
            "name": workflow.name,
            "description": workflow.description,
            "steps_json": _steps_json(workflow),
            "status": workflow.status.value,
            "current_step_index": workflow.current_step_index,
            "created_at": workflow.created_at.isoformat(),
            "updated_at": workflow.updated_at.isoformat(),
            "created_by": workflow.created_by,
        },
    )
    logger.info("Workflow created", workflow_id=workflow.id, name=request.name, steps=len(steps))
    return _workflow_model(node)


@app.get("/workflows", response_model=List[Workflow])
async def list_workflows(status: Optional[str] = None):
    """List workflows with optional status filter."""
    nodes = await store.list_nodes(WORKFLOWS[0], WORKFLOWS[1], {"status": status})
    return [_workflow_model(node) for node in nodes]


@app.get("/workflows/{workflow_id}", response_model=Workflow)
async def get_workflow(workflow_id: str):
    """Get a specific workflow."""
    node = await store.get_node(WORKFLOWS[0], WORKFLOWS[1], workflow_id)
    if not node:
        raise HTTPException(status_code=404, detail="Workflow not found")
    return _workflow_model(node)


@app.post("/workflows/{workflow_id}/execute")
async def execute_workflow(workflow_id: str):
    """Execute a workflow's steps sequentially."""
    node = await store.get_node(WORKFLOWS[0], WORKFLOWS[1], workflow_id)
    if not node:
        raise HTTPException(status_code=404, detail="Workflow not found")
    workflow = _workflow_model(node)
    if workflow.status not in (WorkflowStatus.pending, WorkflowStatus.failed):
        raise HTTPException(status_code=400, detail=f"Workflow cannot be executed from {workflow.status} state")

    workflow.status = WorkflowStatus.running
    workflow.updated_at = datetime.now(timezone.utc)
    logger.info("Workflow execution started", workflow_id=workflow_id)

    # Simulate step execution
    for i, step in enumerate(workflow.steps):
        if step.status in (StepStatus.completed, StepStatus.skipped):
            continue
        step.status = StepStatus.running
        step.started_at = datetime.now(timezone.utc)
        step.status = StepStatus.completed
        step.completed_at = datetime.now(timezone.utc)
        workflow.current_step_index = i
        logger.info("Workflow step completed", workflow_id=workflow_id, step=step.name, index=i)

    workflow.status = WorkflowStatus.completed
    workflow.updated_at = datetime.now(timezone.utc)
    await store.update_node(
        WORKFLOWS[0],
        WORKFLOWS[1],
        workflow_id,
        {
            "status": workflow.status.value,
            "steps_json": _steps_json(workflow),
            "current_step_index": workflow.current_step_index,
            "updated_at": workflow.updated_at.isoformat(),
        },
    )
    return {"workflow_id": workflow_id, "status": "completed", "steps_completed": len(workflow.steps)}


@app.post("/workflows/{workflow_id}/compensate")
async def compensate_workflow(workflow_id: str):
    """Run compensation actions for completed steps (rollback)."""
    node = await store.get_node(WORKFLOWS[0], WORKFLOWS[1], workflow_id)
    if not node:
        raise HTTPException(status_code=404, detail="Workflow not found")
    workflow = _workflow_model(node)

    workflow.status = WorkflowStatus.compensated
    workflow.updated_at = datetime.now(timezone.utc)
    for step in reversed(workflow.steps):
        if step.status == StepStatus.completed and step.compensation_url:
            step.status = StepStatus.compensated
            logger.info("Step compensated", workflow_id=workflow_id, step=step.name)

    await store.update_node(
        WORKFLOWS[0],
        WORKFLOWS[1],
        workflow_id,
        {
            "status": workflow.status.value,
            "steps_json": _steps_json(workflow),
            "updated_at": workflow.updated_at.isoformat(),
        },
    )
    return {"workflow_id": workflow_id, "status": "compensated"}


@app.delete("/workflows/{workflow_id}")
async def cancel_workflow(workflow_id: str):
    """Cancel a running workflow."""
    node = await store.get_node(WORKFLOWS[0], WORKFLOWS[1], workflow_id)
    if not node:
        raise HTTPException(status_code=404, detail="Workflow not found")
    workflow = _workflow_model(node)

    workflow.status = WorkflowStatus.cancelled
    workflow.updated_at = datetime.now(timezone.utc)
    await store.update_node(
        WORKFLOWS[0],
        WORKFLOWS[1],
        workflow_id,
        {"status": workflow.status.value, "updated_at": workflow.updated_at.isoformat()},
    )
    return {"workflow_id": workflow_id, "status": "cancelled"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
