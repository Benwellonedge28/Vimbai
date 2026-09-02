"""
Vimbai Distributed Workflow Orchestration Service
Orchestrates multi-step workflows across distributed services with retry and compensation.
"""

import os
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "distributed-workflow-orchestration-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8415"))

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


workflows: List[Workflow] = []


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
    workflows.append(workflow)
    logger.info("Workflow created", workflow_id=workflow.id, name=request.name, steps=len(steps))
    return workflow


@app.get("/workflows", response_model=List[Workflow])
async def list_workflows(status: Optional[str] = None):
    """List workflows with optional status filter."""
    if status:
        return [w for w in workflows if w.status.value == status]
    return workflows


@app.get("/workflows/{workflow_id}", response_model=Workflow)
async def get_workflow(workflow_id: str):
    """Get a specific workflow."""
    workflow = next((w for w in workflows if w.id == workflow_id), None)
    if not workflow:
        raise HTTPException(status_code=404, detail="Workflow not found")
    return workflow


@app.post("/workflows/{workflow_id}/execute")
async def execute_workflow(workflow_id: str):
    """Execute a workflow's steps sequentially."""
    workflow = next((w for w in workflows if w.id == workflow_id), None)
    if not workflow:
        raise HTTPException(status_code=404, detail="Workflow not found")
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
    return {"workflow_id": workflow_id, "status": "completed", "steps_completed": len(workflow.steps)}


@app.post("/workflows/{workflow_id}/compensate")
async def compensate_workflow(workflow_id: str):
    """Run compensation actions for completed steps (rollback)."""
    workflow = next((w for w in workflows if w.id == workflow_id), None)
    if not workflow:
        raise HTTPException(status_code=404, detail="Workflow not found")

    workflow.status = WorkflowStatus.compensated
    workflow.updated_at = datetime.now(timezone.utc)
    for step in reversed(workflow.steps):
        if step.status == StepStatus.completed and step.compensation_url:
            step.status = StepStatus.compensated
            logger.info("Step compensated", workflow_id=workflow_id, step=step.name)

    return {"workflow_id": workflow_id, "status": "compensated"}


@app.delete("/workflows/{workflow_id}")
async def cancel_workflow(workflow_id: str):
    """Cancel a running workflow."""
    workflow = next((w for w in workflows if w.id == workflow_id), None)
    if not workflow:
        raise HTTPException(status_code=404, detail="Workflow not found")

    workflow.status = WorkflowStatus.cancelled
    workflow.updated_at = datetime.now(timezone.utc)
    return {"workflow_id": workflow_id, "status": "cancelled"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
