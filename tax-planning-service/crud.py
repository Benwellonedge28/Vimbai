"""
Tax Planning Service CRUD Operations

Submitted tax strategies persist as :TaxStrategy nodes via
:OWNS_STRATEGY edges, book_id stamped, Book-gated reads. /plan stays a
pure computation over the request payload (planning results are not
stored, matching the original semantics).
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List

from neo4j import AsyncSession
from tax_planning_service.dependencies import book_id_var
from tax_planning_service.models import TaxStrategy

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _strategy_from_node(n: Dict[str, Any]) -> TaxStrategy:
    return TaxStrategy(
        id=n["id"],
        name=n["name"],
        description=n["description"],
        strategy_type=n["strategy_type"],
        estimated_savings=float(n.get("estimated_savings", 0)),
        implementation_cost=float(n.get("implementation_cost", 0)),
        risk_level=n.get("risk_level", "low"),
        timeframe=n.get("timeframe", "short-term"),
    )


async def create_strategy(session: AsyncSession, user_id: str, strategy: TaxStrategy) -> TaxStrategy:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:TaxStrategy {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        strategy_type: $strategy_type,
        estimated_savings: toFloat($estimated_savings),
        implementation_cost: toFloat($implementation_cost),
        risk_level: $risk_level,
        timeframe: $timeframe,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_STRATEGY]->(x)
    RETURN x
    """
    params = {
        "id": strategy.id or str(uuid.uuid4()),
        "user_id": user_id,
        "name": strategy.name,
        "description": strategy.description,
        "strategy_type": strategy.strategy_type,
        "estimated_savings": strategy.estimated_savings,
        "implementation_cost": strategy.implementation_cost,
        "risk_level": strategy.risk_level,
        "timeframe": strategy.timeframe,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return strategy


async def list_strategies(session: AsyncSession, user_id: str) -> List[TaxStrategy]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_STRATEGY]->(x:TaxStrategy)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at DESC
    """
    result = await _run(session, query, user_id=user_id)
    return [_strategy_from_node(dict(r["x"])) async for r in result]
