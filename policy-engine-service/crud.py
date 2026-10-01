"""
Policy Engine Service CRUD Operations

Rules move from an in-memory _rules defaultdict to Neo4j: :PolicyRule
nodes via :OWNS_RULE edges, book_id stamped, Book-gated reads. The
condition_value is untyped (Any), so it is stored as a JSON prop. Rule
evaluation stays a pure Python computation over the caller's
Book-visible rule set.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List

from neo4j import AsyncSession
from policy_engine_service.dependencies import book_id_var
from policy_engine_service.models import PolicyRule

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _rule_from_node(n: Dict[str, Any], user_id: str) -> PolicyRule:
    return PolicyRule(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        name=n["name"],
        description=n.get("description", ""),
        resource_type=n["resource_type"],
        condition_field=n["condition_field"],
        condition_operator=n.get("condition_operator", ">"),
        condition_value=json.loads(n.get("condition_value_json") or "null"),
        action=n.get("action", "warn"),
        message=n.get("message", ""),
        enabled=bool(n.get("enabled", True)),
    )


async def create_rule(session: AsyncSession, user_id: str, company_id: str, rule: PolicyRule) -> PolicyRule:
    stored = PolicyRule(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        name=rule.name,
        description=rule.description,
        resource_type=rule.resource_type,
        condition_field=rule.condition_field,
        condition_operator=rule.condition_operator,
        condition_value=rule.condition_value,
        action=rule.action,
        message=rule.message,
        enabled=rule.enabled,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PolicyRule {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        name: $name,
        description: $description,
        resource_type: $resource_type,
        condition_field: $condition_field,
        condition_operator: $condition_operator,
        condition_value_json: $condition_value_json,
        action: $action,
        message: $message,
        enabled: $enabled,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_RULE]->(x)
    RETURN x
    """
    params = {
        "id": stored.id,
        "user_id": user_id,
        "company_id": company_id,
        "name": stored.name,
        "description": stored.description,
        "resource_type": stored.resource_type,
        "condition_field": stored.condition_field,
        "condition_operator": stored.condition_operator,
        "condition_value_json": json.dumps(stored.condition_value),
        "action": stored.action.value,
        "message": stored.message,
        "enabled": stored.enabled,
        "created_at": datetime.now(timezone.utc).isoformat(),
    }
    await _run(session, query, params)
    return stored


async def get_rules(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RULE]->(x:PolicyRule {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    rules = [_rule_from_node(dict(r["x"]), user_id) async for r in result]
    return {"company_id": company_id, "rules": rules, "total": len(rules)}


async def get_rules_for_evaluation(
    session: AsyncSession, user_id: str, company_id: str, resource_type: str
) -> List[PolicyRule]:
    data = await get_rules(session, user_id, company_id)
    return [r for r in data["rules"] if r.enabled and r.resource_type == resource_type]
