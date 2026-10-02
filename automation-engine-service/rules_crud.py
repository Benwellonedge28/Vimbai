"""
Automation Engine rule + execution CRUD Operations

Rules and workflow executions move from the in-memory _rules /
_executions dicts to Neo4j: :AutomationRule nodes via :OWNS_RULE
edges and :WorkflowExecution nodes via :OWNS_EXECUTION edges,
book_id stamped, Book-gated. Nested collections (rule steps,
execution step_results) ride on the nodes as JSON props (the
established pattern). Toggle/delete semantics and the workflow
engine (dependency validation, step completion) are unchanged;
executions persist their final status so results survive restarts.
"""

import json
from typing import Any, Dict, List, Optional

from automation_engine_service.dependencies import book_id_var
from automation_engine_service.exceptions import NotFoundError, ValidationError
from automation_engine_service.models import (
    AutomationRule,
    TriggerType,
    WorkflowExecution,
    WorkflowStatus,
    WorkflowStep,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _rule_from_node(n: Dict[str, Any]) -> AutomationRule:
    steps = n.get("steps", "[]")
    if isinstance(steps, str):
        try:
            steps = json.loads(steps)
        except ValueError:
            steps = []
    steps = [WorkflowStep(**s) for s in steps] if steps else []
    return AutomationRule(
        id=n["id"],
        name=n["name"],
        company_id=n["company_id"],
        trigger=n.get("trigger", TriggerType.MANUAL.value),
        condition=(
            json.loads(n["condition"])
            if isinstance(n.get("condition"), str) and n.get("condition")
            else (n.get("condition") or {})
        ),
        steps=steps,
        enabled=bool(n.get("enabled", True)),
        priority=int(n.get("priority", 5)),
    )


def _execution_from_node(n: Dict[str, Any]) -> WorkflowExecution:
    step_results = n.get("step_results", "[]")
    if isinstance(step_results, str):
        try:
            step_results = json.loads(step_results)
        except ValueError:
            step_results = []
    return WorkflowExecution(
        id=n["id"],
        rule_id=n["rule_id"],
        company_id=n["company_id"],
        status=n.get("status", WorkflowStatus.RUNNING.value),
        started_at=n.get("started_at", ""),
        completed_at=n.get("completed_at"),
        step_results=step_results or [],
        error=n.get("error"),
    )


async def create_rule(session: AsyncSession, user_id: str, rule: AutomationRule) -> AutomationRule:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:AutomationRule {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        name: $name,
        trigger: $trigger,
        condition: $condition,
        steps: $steps,
        enabled: $enabled,
        priority: $priority
    }})
    CREATE (u)-[:OWNS_RULE]->(x)
    """
    await _run(
        session,
        query,
        id=rule.id,
        user_id=user_id,
        company_id=rule.company_id,
        name=rule.name,
        trigger=rule.trigger.value if isinstance(rule.trigger, TriggerType) else str(rule.trigger),
        condition=json.dumps(rule.condition),
        steps=json.dumps([s.model_dump(mode="json") for s in rule.steps]),
        enabled=rule.enabled,
        priority=rule.priority,
    )
    return rule


async def list_rules(session: AsyncSession, user_id: str, company_id: str = "") -> List[AutomationRule]:
    if company_id:
        query = f"""
        MATCH (u:User {{id: $user_id}})-[:OWNS_RULE]->(x:AutomationRule {{company_id: $company_id}})
        {BOOK_FILTER}
        RETURN x
        """
        result = await _run(session, query, user_id=user_id, company_id=company_id)
    else:
        query = f"""
        MATCH (u:User {{id: $user_id}})-[:OWNS_RULE]->(x:AutomationRule)
        {BOOK_FILTER}
        RETURN x
        """
        result = await _run(session, query, user_id=user_id)
    return [_rule_from_node(dict(r["x"])) async for r in result]


async def find_rule(session: AsyncSession, user_id: str, rule_id: str) -> Optional[AutomationRule]:
    """Return the rule if the caller owns it and it is visible in this Book."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RULE]->(x:AutomationRule {{id: $rule_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, rule_id=rule_id)
    records = [r async for r in result]
    if not records:
        return None
    return _rule_from_node(dict(records[0]["x"]))


async def delete_rule(session: AsyncSession, user_id: str, rule_id: str) -> bool:
    """Delete a caller-owned rule; returns False when invisible (original semantics)."""
    rule = await find_rule(session, user_id, rule_id)
    if rule is None:
        return False
    query = """
    MATCH (x:AutomationRule {id: $rule_id})
    DETACH DELETE x
    """
    await _run(session, query, rule_id=rule_id)
    return True


async def toggle_rule(session: AsyncSession, user_id: str, rule_id: str) -> bool:
    """Flip a caller-owned rule's enabled flag; raises NotFound when invisible."""
    rule = await find_rule(session, user_id, rule_id)
    if rule is None:
        raise NotFoundError("Rule not found")
    query = "MATCH (x:AutomationRule {id: $rule_id}) SET x.enabled = $enabled"
    await _run(session, query, rule_id=rule_id, enabled=not rule.enabled)
    return not rule.enabled


async def create_execution(
    session: AsyncSession, user_id: str, execution: WorkflowExecution, rule: AutomationRule
) -> WorkflowExecution:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:WorkflowExecution {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        rule_id: $rule_id,
        status: $status,
        started_at: $started_at,
        completed_at: $completed_at,
        step_results: $step_results,
        error: $error
    }})
    CREATE (u)-[:OWNS_EXECUTION]->(x)
    """
    params = {
        "id": execution.id,
        "user_id": user_id,
        "company_id": execution.company_id,
        "rule_id": execution.rule_id,
        "status": execution.status.value,
        "started_at": execution.started_at,
        "completed_at": execution.completed_at,
        "step_results": json.dumps(execution.step_results),
        "error": execution.error,
    }
    await _run(session, query, params, rule_id=execution.rule_id)
    return execution


async def get_execution(session: AsyncSession, user_id: str, execution_id: str) -> Optional[WorkflowExecution]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_EXECUTION]->(x:WorkflowExecution {{id: $execution_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, execution_id=execution_id)
    records = [r async for r in result]
    if not records:
        return None
    return _execution_from_node(dict(records[0]["x"]))


async def list_executions(
    session: AsyncSession, user_id: str, company_id: str = "", status: str = ""
) -> List[WorkflowExecution]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_EXECUTION]->(x:WorkflowExecution)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    executions = [_execution_from_node(dict(r["x"])) async for r in result]
    if company_id:
        executions = [e for e in executions if e.company_id == company_id]
    if status:
        executions = [e for e in executions if e.status.value == status]
    return executions


async def persist_execution_state(session: AsyncSession, execution: WorkflowExecution) -> None:
    """Write back an execution's final state (status, results, error)."""
    query = """
    MATCH (x:WorkflowExecution {id: $id})
    SET x.status = $status,
        x.completed_at = $completed_at,
        x.step_results = $step_results,
        x.error = $error
    """
    await _run(
        session,
        query,
        id=execution.id,
        status=execution.status.value if isinstance(execution.status, WorkflowStatus) else str(execution.status),
        completed_at=execution.completed_at,
        step_results=json.dumps(execution.step_results),
        error=execution.error,
    )
