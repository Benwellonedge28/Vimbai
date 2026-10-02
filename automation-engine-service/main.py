"""
Vimbai Automation Engine Service
Workflow orchestration, rule-based automation, and scheduled task execution.
Port: 8006

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "automation_engine_service" not in _sys.modules or not hasattr(
    _sys.modules.get("automation_engine_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("automation_engine_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["automation_engine_service"] = _pkg
    _sys.modules["automation_engine_service"].__path__ = [_HERE]

import os
from datetime import datetime, timezone
from typing import List

import structlog
from automation_engine_service import models, rules_crud
from automation_engine_service.dependencies import book_id_var, get_db_session, get_user_id
from automation_engine_service.exceptions import AutomationEngineError
from automation_engine_service.models import AutomationRule, TriggerType, WorkflowExecution, WorkflowStatus
from fastapi import BackgroundTasks, Depends, FastAPI, Header, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "automation-engine-service"
PORT = int(os.getenv("PORT", "8006"))
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
app = FastAPI(title="Vimbai Automation Engine Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(AutomationEngineError)
async def _automation_engine_error(request: Request, exc: AutomationEngineError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


async def _count(session: AsyncSession, user_id: str, label: str) -> int:
    """Count the caller's Book-visible nodes of a label (metrics)."""
    label_node = {"RULE": "AutomationRule", "EXECUTION": "WorkflowExecution"}[label]
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_{label}]->(x:{label_node})
    WHERE ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await rules_crud._run(session, query, user_id=user_id)
    return len([r async for r in result])


@app.get("/")
@app.get("/health")
async def health(
    x_user_id: str = Header(default=""),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Health check; includes the caller's rule/execution counts when authenticated."""
    body = {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}
    if x_user_id:
        body["rules"] = await _count(db_session, x_user_id, "RULE")
        body["executions"] = await _count(db_session, x_user_id, "EXECUTION")
    return body


@app.post("/rules", response_model=AutomationRule)
async def create_rule(
    rule: AutomationRule,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    created = await rules_crud.create_rule(db_session, user_id, rule)
    logger.info("Rule created", rule_id=created.id, name=created.name)
    return created


@app.get("/rules", response_model=List[AutomationRule])
async def list_rules(
    company_id: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's Book-visible rules (optionally narrowed by company)."""
    return await rules_crud.list_rules(db_session, user_id, company_id)


@app.get("/rules/{rule_id}", response_model=AutomationRule)
async def get_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    rule = await rules_crud.find_rule(db_session, user_id, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Rule not found")
    return rule


@app.delete("/rules/{rule_id}")
async def delete_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a caller-owned rule; unknown/invisible ids return deleted: False (original semantics)."""
    deleted = await rules_crud.delete_rule(db_session, user_id, rule_id)
    return {"deleted": deleted, "rule_id": rule_id}


@app.post("/rules/{rule_id}/toggle")
async def toggle_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Flip a caller-owned rule's enabled flag; cross-scope toggles 404."""
    enabled = await rules_crud.toggle_rule(db_session, user_id, rule_id)
    return {"rule_id": rule_id, "enabled": enabled}


@app.post("/execute/{rule_id}", response_model=WorkflowExecution)
async def execute_rule(
    rule_id: str,
    background_tasks: BackgroundTasks,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Start a workflow execution for a caller-owned, Book-visible rule."""
    rule = await rules_crud.find_rule(db_session, user_id, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Rule not found")
    if not rule.enabled:
        raise HTTPException(status_code=400, detail="Rule is disabled")

    execution = WorkflowExecution(rule_id=rule_id, company_id=rule.company_id, status=WorkflowStatus.RUNNING)
    await rules_crud.create_execution(db_session, user_id, execution, rule)

    background_tasks.add_task(_run_workflow, execution.id, rule, user_id)
    return execution


async def _run_workflow(execution_id: str, rule: AutomationRule, user_id: str):
    """Run the rule's steps (dependency validation preserved) and persist the final state."""
    from automation_engine_service.database import Neo4jConnector

    async with Neo4jConnector.get_driver().session() as session:
        execution = await rules_crud.get_execution(session, user_id, execution_id)
        if execution is None:
            return
        completed_steps = set()

        for step in rule.steps:
            if step.depends_on:
                for dep in step.depends_on:
                    if dep not in completed_steps:
                        execution.status = WorkflowStatus.FAILED
                        execution.error = f"Dependency {dep} not completed for step {step.step_id}"
                        execution.completed_at = datetime.now(timezone.utc).isoformat()
                        await rules_crud.persist_execution_state(session, execution)
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
        await rules_crud.persist_execution_state(session, execution)
        logger.info("Workflow completed", execution_id=execution_id, steps=len(completed_steps))


@app.get("/executions", response_model=List[WorkflowExecution])
async def list_executions(
    company_id: str = "",
    status: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's Book-visible executions (optionally narrowed by company/status)."""
    return await rules_crud.list_executions(db_session, user_id, company_id, status)


@app.get("/executions/{execution_id}", response_model=WorkflowExecution)
async def get_execution(
    execution_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    execution = await rules_crud.get_execution(db_session, user_id, execution_id)
    if execution is None:
        raise HTTPException(status_code=404, detail="Execution not found")
    return execution


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
