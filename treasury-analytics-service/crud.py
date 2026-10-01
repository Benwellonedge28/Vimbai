"""
Treasury Analytics Service CRUD Operations

Analyzed snapshots move from the in-memory _metrics dict (keyed by
company_id, latest overwrite) to Neo4j: :TreasuryAnalyticsSnapshot
nodes via :OWNS_SNAPSHOT edges, book_id stamped, Book-gated. KPI
computation itself stays a pure calculation in main.py; /kpi/{id}
returns the caller's latest Book-visible snapshot for the company.
"""

import json
import uuid
from typing import Any, Dict, Optional

from neo4j import AsyncSession
from treasury_analytics_service.dependencies import book_id_var
from treasury_analytics_service.models import AnalyticsResponse

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _snapshot_from_node(n: Dict[str, Any]) -> AnalyticsResponse:
    return AnalyticsResponse(
        company_id=n["company_id"],
        kpis=json.loads(n.get("kpis", "[]") or "[]"),
        cash_adequacy_days=float(n.get("cash_adequacy_days", 0)),
        debt_service_ratio=float(n.get("debt_service_ratio", 0)),
        investment_yield=float(n.get("investment_yield", 0)),
        fx_risk_score=float(n.get("fx_risk_score", 0)),
    )


async def save_snapshot(session: AsyncSession, user_id: str, resp: AnalyticsResponse) -> None:
    """Replace the caller's previous snapshot for the company with the fresh analysis."""
    # remove prior snapshots for this company (latest-wins, matching the old dict overwrite)
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SNAPSHOT]->(x:TreasuryAnalyticsSnapshot {{company_id: $company_id}})
    {BOOK_FILTER}
    DETACH DELETE x
    """
    await _run(session, query, user_id=user_id, company_id=resp.company_id)
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:TreasuryAnalyticsSnapshot {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        kpis: $kpis,
        cash_adequacy_days: toFloat($cash_adequacy_days),
        debt_service_ratio: toFloat($debt_service_ratio),
        investment_yield: toFloat($investment_yield),
        fx_risk_score: toFloat($fx_risk_score)
    }})
    CREATE (u)-[:OWNS_SNAPSHOT]->(x)
    """
    await _run(
        session,
        query,
        id=str(uuid.uuid4()),
        user_id=user_id,
        company_id=resp.company_id,
        kpis=json.dumps([k.model_dump() for k in resp.kpis]),
        cash_adequacy_days=resp.cash_adequacy_days,
        debt_service_ratio=resp.debt_service_ratio,
        investment_yield=resp.investment_yield,
        fx_risk_score=resp.fx_risk_score,
    )


async def get_snapshot(session: AsyncSession, user_id: str, company_id: str) -> Optional[AnalyticsResponse]:
    """Return the caller's latest Book-visible snapshot for the company, if any."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SNAPSHOT]->(x:TreasuryAnalyticsSnapshot {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    records = [r async for r in result]
    if not records:
        return None
    return _snapshot_from_node(dict(records[0]["x"]))
