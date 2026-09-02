"""
Vimbai Automation Engine Service
Workflow orchestration, rule-based automation, and scheduled task execution.
Port: 8006
"""

import asyncio
import os
import time
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import BackgroundTasks, FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "automation-engine-service"
PORT = int(os.getenv("PORT", "8006"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Automation Engine Service", version="2.0.0", docs_url="/docs")
# Distributed tracing
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class TriggerType(str, Enum):
    SCHEDULED = "scheduled"
    EVENT = "event"
    MANUAL = "manual"
    WEBHOOK = "webhook"


class WorkflowStatus(str, Enum):
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    PAUSED = "paused"


class WorkflowStep(BaseModel):
    step_id: str
    step_name: str
    action: str
    params: Dict[str, Any] = {}
    depends_on: List[str] = []
    timeout_seconds: int = 300


class AutomationRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    company_id: str
    trigger: TriggerType
    condition: Dict[str, Any] = {}
    steps: List[WorkflowStep] = []
    enabled: bool = True
    priority: int = 5


class WorkflowExecution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    rule_id: str
    company_id: str
    status: WorkflowStatus
    started_at: str = Field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    completed_at: Optional[str] = None
    step_results: List[Dict] = []
    error: Optional[str] = None


_rules: Dict[str, AutomationRule] = {}
_executions: Dict[str, WorkflowExecution] = {}


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "rules": len(_rules),
        "executions": len(_executions),
    }


@app.post("/rules", response_model=AutomationRule)
async def create_rule(rule: AutomationRule):
    _rules[rule.id] = rule
    logger.info("Rule created", rule_id=rule.id, name=rule.name)
    return rule


@app.get("/rules", response_model=List[AutomationRule])
async def list_rules(company_id: str = ""):
    if company_id:
        return [r for r in _rules.values() if r.company_id == company_id]
    return list(_rules.values())


@app.get("/rules/{rule_id}", response_model=AutomationRule)
async def get_rule(rule_id: str):
    if rule_id not in _rules:
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Rule not found")
    return _rules[rule_id]


@app.delete("/rules/{rule_id}")
async def delete_rule(rule_id: str):
    if rule_id in _rules:
        del _rules[rule_id]
        return {"deleted": True, "rule_id": rule_id}
    return {"deleted": False, "rule_id": rule_id}


@app.post("/rules/{rule_id}/toggle")
async def toggle_rule(rule_id: str):
    if rule_id in _rules:
        _rules[rule_id].enabled = not _rules[rule_id].enabled
        return {"rule_id": rule_id, "enabled": _rules[rule_id].enabled}
    from fastapi import HTTPException

    raise HTTPException(status_code=404, detail="Rule not found")


@app.post("/execute/{rule_id}", response_model=WorkflowExecution)
async def execute_rule(rule_id: str, background_tasks: BackgroundTasks):
    if rule_id not in _rules:
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Rule not found")

    rule = _rules[rule_id]
    if not rule.enabled:
        from fastapi import HTTPException

        raise HTTPException(status_code=400, detail="Rule is disabled")

    execution = WorkflowExecution(rule_id=rule_id, company_id=rule.company_id, status=WorkflowStatus.RUNNING)
    _executions[execution.id] = execution

    background_tasks.add_task(_run_workflow, execution.id, rule)
    return execution


async def _run_workflow(execution_id: str, rule: AutomationRule):
    execution = _executions[execution_id]
    completed_steps = set()

    for step in rule.steps:
        if step.depends_on:
            for dep in step.depends_on:
                if dep not in completed_steps:
                    execution.status = WorkflowStatus.FAILED
                    execution.error = f"Dependency {dep} not completed for step {step.step_id}"
                    execution.completed_at = datetime.now(timezone.utc).isoformat()
                    return

        result = {
            "step_id": step.step_id,
            "step_name": step.step_name,
            "action": step.action,
            "status": "completed",
            "params": step.params,
        }
        execution.step_results.append(result)
        completed_steps.add(step.step_id)

    execution.status = WorkflowStatus.COMPLETED
    execution.completed_at = datetime.now(timezone.utc).isoformat()
    logger.info("Workflow completed", execution_id=execution_id, steps=len(completed_steps))


@app.get("/executions", response_model=List[WorkflowExecution])
async def list_executions(company_id: str = "", status: str = ""):
    results = list(_executions.values())
    if company_id:
        results = [e for e in results if e.company_id == company_id]
    if status:
        results = [e for e in results if e.status.value == status]
    return results


@app.get("/executions/{execution_id}", response_model=WorkflowExecution)
async def get_execution(execution_id: str):
    if execution_id not in _executions:
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Execution not found")
    return _executions[execution_id]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
