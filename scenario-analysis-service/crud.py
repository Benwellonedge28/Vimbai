"""
Scenario Analysis Service CRUD Operations

The /analyze best/base/worst modeling stays a pure computation over the
submitted assumptions (no persistence in the original mock either).
Stored scenarios (the /scenarios CRUD) move to Neo4j: caller-owned via
:OWNS_SCENARIO edges, book_id stamped, every read Book-gated.
"""

import uuid
from typing import Any, Dict, List

from neo4j import AsyncSession
from scenario_analysis_service.dependencies import book_id_var
from scenario_analysis_service.exceptions import NotFoundError
from scenario_analysis_service.models import Scenario, ScenarioCreate

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _scenario_from_node(n: Dict[str, Any], user_id: str) -> Scenario:
    return Scenario(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        name=n["name"],
        scenario_type=n.get("scenario_type", "custom"),
        projected_revenue=float(n.get("projected_revenue", 0)),
        projected_expenses=float(n.get("projected_expenses", 0)),
        net_projection=float(n.get("net_projection", 0)),
    )


async def _list_scenarios(session: AsyncSession, user_id: str, company_id: str) -> List[Scenario]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCENARIO]->(x:Scenario {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_scenario_from_node(dict(r["x"]), user_id) async for r in result]


async def create_scenario(session: AsyncSession, user_id: str, payload: ScenarioCreate) -> Scenario:
    scenario = Scenario(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=payload.company_id,
        name=payload.name,
        scenario_type=payload.scenario_type,
        projected_revenue=payload.projected_revenue,
        projected_expenses=payload.projected_expenses,
        net_projection=payload.projected_revenue - payload.projected_expenses,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Scenario {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        name: $name,
        scenario_type: $scenario_type,
        projected_revenue: toFloat($projected_revenue),
        projected_expenses: toFloat($projected_expenses),
        net_projection: toFloat($net_projection),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_SCENARIO]->(x)
    RETURN x
    """
    from datetime import datetime, timezone

    params = {
        "id": scenario.id,
        "user_id": user_id,
        "company_id": scenario.company_id,
        "name": scenario.name,
        "scenario_type": scenario.scenario_type,
        "projected_revenue": scenario.projected_revenue,
        "projected_expenses": scenario.projected_expenses,
        "net_projection": scenario.net_projection,
        "created_at": datetime.now(timezone.utc).isoformat(),
    }
    await _run(session, query, params)
    return scenario


async def list_scenarios(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    items = await _list_scenarios(session, user_id, company_id)
    return {"total": len(items), "scenarios": items}


async def compare_scenarios(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    items = await _list_scenarios(session, user_id, company_id)
    if len(items) < 2:
        return {"comparison": "Need at least 2 scenarios", "best_case": "", "worst_case": ""}
    best = max(items, key=lambda s: s.net_projection)
    worst = min(items, key=lambda s: s.net_projection)
    return {
        "best_case": best.name,
        "worst_case": worst.name,
        "best_net": best.net_projection,
        "worst_net": worst.net_projection,
        "range": best.net_projection - worst.net_projection,
    }
