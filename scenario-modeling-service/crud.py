"""
Scenario Modeling Service CRUD Operations

Scenarios, modeling rules, What-If results and sensitivity results
move from the in-memory Storage class (class-level dicts shared by
every request) to Neo4j:

- :ScenarioModel nodes via :OWNS_SCENARIO_MODEL edges
- :ModelingRule nodes via :OWNS_MODELING_RULE edges
- :WhatIfModelResult nodes via :OWNS_WHATIF_RESULT edges
- :SensitivityModelResult nodes via :OWNS_SENSITIVITY_RESULT edges

All nodes are stamped user_id + book_id and reads are Book-gated
($book_id IS NULL OR x.book_id = $book_id). Nested collections
(variables, conditions, actions, assumptions, outcomes,
recommendations) ride on the nodes as JSON props, the established
pattern. Labels are suffixed "Model*" to stay distinct from the
scenario-analysis / sensitivity-analysis services' nodes when the
services share one production Neo4j instance.

Security fix carried by this move: previously the rules endpoints
had NO user dimension at all (any caller created, mutated and
deleted rules shared by everyone), and What-If / sensitivity /
compare endpoints ran against ANY scenario id regardless of who
created it. Everything is now caller-owned and Book-gated; cross-
scope reads and mutations 404 like the rest of the rollout.

The analysis computations themselves (calculate_outcome, rule
evaluation, sensitivity stepping) are pure functions in main.py
and unchanged - only storage and visibility change.
"""

import json
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from scenario_modeling_service import models
from scenario_modeling_service.dependencies import book_id_var

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _j(value: Any) -> str:
    """JSON-encode with Decimals as strings (they round-trip into pydantic)."""
    return json.dumps(value, default=str)


def _load(name: str, n: Dict[str, Any], default: Any) -> Any:
    raw = n.get(name, default)
    if isinstance(raw, str):
        try:
            return json.loads(raw)
        except (ValueError, TypeError):
            return default
    return raw if raw is not None else default


def _coerce_dt(value: Any) -> datetime:
    if isinstance(value, datetime):
        return value
    if value is None:
        return datetime.now(timezone.utc)
    if hasattr(value, "iso_format"):
        try:
            return datetime.fromisoformat(value.iso_format())
        except (TypeError, ValueError):
            return datetime.now(timezone.utc)
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return datetime.now(timezone.utc)
    return datetime.now(timezone.utc)


# ---------------------------------------------------------------------------
# Scenarios
# ---------------------------------------------------------------------------


def _scenario_from_node(n: Dict[str, Any]) -> Any:
    variables = _load("variables", n, [])
    return models.ScenarioInDB(
        id=n["id"],
        user_id=n["user_id"],
        name=n["name"],
        description=n.get("description"),
        scenario_type=n.get("scenario_type", models.ScenarioType.CUSTOM.value),
        base_date=n.get("base_date"),
        end_date=n.get("end_date"),
        variables=[models.ScenarioVariable(**v) for v in variables],
        rules=_load("rules", n, []),
        assumptions=_load("assumptions", n, None),
        status=n.get("status", "draft"),
        created_at=_coerce_dt(n.get("created_at")),
        updated_at=_coerce_dt(n.get("updated_at")),
        results=_load("results", n, None),
    )


async def create_scenario(session: AsyncSession, user_id: str, scenario: Any) -> Any:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:ScenarioModel {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        scenario_type: $scenario_type,
        base_date: $base_date,
        end_date: $end_date,
        status: $status,
        variables: $variables,
        rules: $rules,
        assumptions: $assumptions,
        results: $results,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    }})
    CREATE (u)-[:OWNS_SCENARIO_MODEL]->(x)
    """
    await _run(
        session,
        query,
        id=scenario.id,
        user_id=user_id,
        name=scenario.name,
        description=scenario.description,
        scenario_type=(
            scenario.scenario_type.value if hasattr(scenario.scenario_type, "value") else str(scenario.scenario_type)
        ),
        base_date=scenario.base_date.isoformat(),
        end_date=scenario.end_date.isoformat(),
        status=scenario.status,
        variables=_j([v.model_dump(mode="json") for v in scenario.variables]),
        rules=_j(scenario.rules),
        assumptions=_j(scenario.assumptions) if scenario.assumptions is not None else None,
        results=_j(scenario.results) if scenario.results is not None else None,
        created_at=scenario.created_at.isoformat(),
        updated_at=scenario.updated_at.isoformat(),
    )
    return scenario


async def list_scenarios(session: AsyncSession, user_id: str, status: str = "", scenario_type: str = "") -> List[Any]:
    """List the caller's Book-visible scenarios, newest first (original order)."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCENARIO_MODEL]->(x:ScenarioModel)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    scenarios = [_scenario_from_node(dict(r["x"])) async for r in result]
    if status:
        scenarios = [s for s in scenarios if s.status == status]
    if scenario_type:
        scenarios = [s for s in scenarios if s.scenario_type == scenario_type]
    return sorted(scenarios, key=lambda s: s.created_at, reverse=True)


async def find_scenario(session: AsyncSession, user_id: str, scenario_id: str) -> Optional[Any]:
    """Return the scenario if the caller owns it and it is visible in this Book."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCENARIO_MODEL]->(x:ScenarioModel {{id: $scenario_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, scenario_id=scenario_id)
    records = [r async for r in result]
    if not records:
        return None
    return _scenario_from_node(dict(records[0]["x"]))


async def update_scenario(session: AsyncSession, user_id: str, scenario: Any) -> None:
    """Persist a mutated scenario (original setattr semantics applied in main.py)."""
    query = """
    MATCH (x:ScenarioModel {id: $id})
    SET x.name = $name,
        x.description = $description,
        x.scenario_type = $scenario_type,
        x.base_date = $base_date,
        x.end_date = $end_date,
        x.status = $status,
        x.variables = $variables,
        x.rules = $rules,
        x.assumptions = $assumptions,
        x.results = $results,
        x.updated_at = $updated_at
    """
    await _run(
        session,
        query,
        id=scenario.id,
        name=scenario.name,
        description=scenario.description,
        scenario_type=(
            scenario.scenario_type.value if hasattr(scenario.scenario_type, "value") else str(scenario.scenario_type)
        ),
        base_date=scenario.base_date.isoformat(),
        end_date=scenario.end_date.isoformat(),
        status=scenario.status,
        variables=_j([v.model_dump(mode="json") for v in scenario.variables]),
        rules=_j(scenario.rules),
        assumptions=_j(scenario.assumptions) if scenario.assumptions is not None else None,
        results=_j(scenario.results) if scenario.results is not None else None,
        updated_at=scenario.updated_at.isoformat(),
    )


async def delete_scenario(session: AsyncSession, user_id: str, scenario_id: str) -> bool:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCENARIO_MODEL]->(x:ScenarioModel {{id: $scenario_id}})
    {BOOK_FILTER}
    DETACH DELETE x
    """
    result = await _run(session, query, user_id=user_id, scenario_id=scenario_id)
    return True


# ---------------------------------------------------------------------------
# Modeling rules
# ---------------------------------------------------------------------------


def _rule_from_node(n: Dict[str, Any]) -> Any:
    return models.ModelingRule(
        id=n["id"],
        name=n["name"],
        description=n.get("description"),
        conditions=[models.RuleCondition(**c) for c in _load("conditions", n, [])],
        actions=[models.RuleAction(**a) for a in _load("actions", n, [])],
        priority=int(n.get("priority", 100)),
        enabled=bool(n.get("enabled", True)),
        created_at=_coerce_dt(n.get("created_at")),
        updated_at=_coerce_dt(n.get("updated_at")),
    )


async def create_rule(session: AsyncSession, user_id: str, rule: Any) -> Any:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:ModelingRule {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        conditions: $conditions,
        actions: $actions,
        priority: $priority,
        enabled: $enabled,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    }})
    CREATE (u)-[:OWNS_MODELING_RULE]->(x)
    """
    await _run(
        session,
        query,
        id=rule.id,
        user_id=user_id,
        name=rule.name,
        description=rule.description,
        conditions=_j([c.model_dump(mode="json") for c in rule.conditions]),
        actions=_j([a.model_dump(mode="json") for a in rule.actions]),
        priority=rule.priority,
        enabled=rule.enabled,
        created_at=rule.created_at.isoformat(),
        updated_at=rule.updated_at.isoformat(),
    )
    return rule


async def list_rules(session: AsyncSession, user_id: str, enabled_only: bool = False) -> List[Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MODELING_RULE]->(x:ModelingRule)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    rules = [_rule_from_node(dict(r["x"])) async for r in result]
    if enabled_only:
        rules = [r for r in rules if r.enabled]
    return rules


async def find_rule(session: AsyncSession, user_id: str, rule_id: str) -> Optional[Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MODELING_RULE]->(x:ModelingRule {{id: $rule_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, rule_id=rule_id)
    records = [r async for r in result]
    if not records:
        return None
    return _rule_from_node(dict(records[0]["x"]))


async def update_rule(session: AsyncSession, user_id: str, rule: Any) -> None:
    """Persist a mutated rule (original setattr semantics applied in main.py)."""
    query = """
    MATCH (x:ModelingRule {id: $id})
    SET x.name = $name,
        x.description = $description,
        x.conditions = $conditions,
        x.actions = $actions,
        x.priority = $priority,
        x.enabled = $enabled,
        x.updated_at = $updated_at
    """
    await _run(
        session,
        query,
        id=rule.id,
        name=rule.name,
        description=rule.description,
        conditions=_j([c.model_dump(mode="json") for c in rule.conditions]),
        actions=_j([a.model_dump(mode="json") for a in rule.actions]),
        priority=rule.priority,
        enabled=rule.enabled,
        updated_at=rule.updated_at.isoformat(),
    )


async def delete_rule(session: AsyncSession, user_id: str, rule_id: str) -> bool:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_MODELING_RULE]->(x:ModelingRule {{id: $rule_id}})
    {BOOK_FILTER}
    DETACH DELETE x
    """
    await _run(session, query, user_id=user_id, rule_id=rule_id)
    return True


# ---------------------------------------------------------------------------
# What-If results
# ---------------------------------------------------------------------------


def _whatif_from_node(n: Dict[str, Any]) -> Any:
    def _dec(d: Dict[str, Any]) -> Dict[str, Decimal]:
        return {k: Decimal(str(v)) for k, v in d.items()}

    return models.WhatIfResult(
        analysis_id=n["id"],
        scenario_id=n["scenario_id"],
        base_values=_dec(_load("base_values", n, {})),
        changed_values=_dec(_load("changed_values", n, {})),
        original_outcome=Decimal(str(n.get("original_outcome", "0"))),
        new_outcome=Decimal(str(n.get("new_outcome", "0"))),
        variance=Decimal(str(n.get("variance", "0"))),
        variance_percent=float(n.get("variance_percent", 0)),
        affected_accounts=_load("affected_accounts", n, []),
        timestamp=_coerce_dt(n.get("timestamp")),
    )


async def create_what_if(session: AsyncSession, user_id: str, result: Any) -> Any:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:WhatIfModelResult {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        scenario_id: $scenario_id,
        base_values: $base_values,
        changed_values: $changed_values,
        original_outcome: $original_outcome,
        new_outcome: $new_outcome,
        variance: $variance,
        variance_percent: toFloat($variance_percent),
        affected_accounts: $affected_accounts,
        timestamp: datetime($timestamp)
    }})
    CREATE (u)-[:OWNS_WHATIF_RESULT]->(x)
    """
    await _run(
        session,
        query,
        id=result.analysis_id,
        user_id=user_id,
        scenario_id=result.scenario_id,
        base_values=_j({k: str(v) for k, v in result.base_values.items()}),
        changed_values=_j({k: str(v) for k, v in result.changed_values.items()}),
        original_outcome=str(result.original_outcome),
        new_outcome=str(result.new_outcome),
        variance=str(result.variance),
        variance_percent=result.variance_percent,
        affected_accounts=_j(result.affected_accounts),
        timestamp=result.timestamp.isoformat(),
    )
    return result


async def get_what_if(session: AsyncSession, user_id: str, analysis_id: str) -> Optional[Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_WHATIF_RESULT]->(x:WhatIfModelResult {{id: $analysis_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, analysis_id=analysis_id)
    records = [r async for r in result]
    if not records:
        return None
    return _whatif_from_node(dict(records[0]["x"]))


async def list_what_if_for_scenario(session: AsyncSession, user_id: str, scenario_id: str) -> List[Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_WHATIF_RESULT]->(x:WhatIfModelResult {{scenario_id: $scenario_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, scenario_id=scenario_id)
    return [_whatif_from_node(dict(r["x"])) async for r in result]


# ---------------------------------------------------------------------------
# Sensitivity results
# ---------------------------------------------------------------------------


def _sensitivity_from_node(n: Dict[str, Any]) -> Any:
    return models.SensitivityResult(
        analysis_id=n["id"],
        variable_name=n["variable_name"],
        outcomes=_load("outcomes", n, []),
        most_sensitive_range=_load("most_sensitive_range", n, {}),
        recommendations=_load("recommendations", n, []),
    )


async def create_sensitivity(session: AsyncSession, user_id: str, scenario_id: str, result: Any) -> Any:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:SensitivityModelResult {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        scenario_id: $scenario_id,
        variable_name: $variable_name,
        outcomes: $outcomes,
        most_sensitive_range: $most_sensitive_range,
        recommendations: $recommendations,
        timestamp: datetime($timestamp)
    }})
    CREATE (u)-[:OWNS_SENSITIVITY_RESULT]->(x)
    """
    await _run(
        session,
        query,
        id=result.analysis_id,
        user_id=user_id,
        scenario_id=scenario_id,
        variable_name=result.variable_name,
        outcomes=_j(result.outcomes),
        most_sensitive_range=_j(result.most_sensitive_range),
        recommendations=_j(result.recommendations),
        timestamp=datetime.now(timezone.utc).isoformat(),
    )
    return result


async def get_sensitivity(session: AsyncSession, user_id: str, analysis_id: str) -> Optional[Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SENSITIVITY_RESULT]->(x:SensitivityModelResult {{id: $analysis_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, analysis_id=analysis_id)
    records = [r async for r in result]
    if not records:
        return None
    return _sensitivity_from_node(dict(records[0]["x"]))
