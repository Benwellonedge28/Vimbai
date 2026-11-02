"""
Cash Flow Service CRUD Operations

Schedules move from a module-level list to Neo4j: :CashFlowSummary nodes via
:OWNS_CASH_FLOW edges, book_id stamped on create, Book-gated reads. The yearly
entries live on the node as a JSON prop; the totals are summed in Python and
written back so a read never depends on node-prop arithmetic.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from cash_flow_service.dependencies import book_id_var
from cash_flow_service.exceptions import NotFoundError
from cash_flow_service.models import CashFlowEntry, CashFlowSummary
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _as_dt(value) -> datetime:
    if value is None:
        return _now()
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return _now()
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _summary_from_node(n: Dict[str, Any], user_id: str) -> CashFlowSummary:
    entries = [CashFlowEntry(**e) for e in json.loads(n.get("entries_json") or "[]")]
    return CashFlowSummary(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        project_name=n["project_name"],
        initial_investment=float(n.get("initial_investment") or 0),
        entries=entries,
        total_inflows=float(n.get("total_inflows") or 0),
        total_outflows=float(n.get("total_outflows") or 0),
        total_net_flow=float(n.get("total_net_flow") or 0),
        created_at=_as_dt(n.get("created_at")),
    )


def build_entries(yearly_inflows: List[float], yearly_outflows: Optional[List[float]]) -> List[CashFlowEntry]:
    """One entry per inflow year, with an outflow where the caller gave one."""
    entries = []
    for i, inflow in enumerate(yearly_inflows):
        outflow = yearly_outflows[i] if yearly_outflows and i < len(yearly_outflows) else 0
        entries.append(CashFlowEntry(year=i + 1, inflow=inflow, outflow=outflow, net_flow=inflow - outflow))
    return entries


async def create_summary(
    session: AsyncSession,
    user_id: str,
    project_name: str,
    initial_investment: float,
    yearly_inflows: List[float],
    yearly_outflows: Optional[List[float]] = None,
) -> CashFlowSummary:
    entries = build_entries(yearly_inflows, yearly_outflows)
    summary = CashFlowSummary(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        project_name=project_name,
        initial_investment=initial_investment,
        entries=entries,
        total_inflows=sum(e.inflow for e in entries),
        total_outflows=sum(e.outflow for e in entries),
        total_net_flow=sum(e.net_flow for e in entries),
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CashFlowSummary {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        project_name: $project_name,
        initial_investment: toFloat($initial_investment),
        entries_json: $entries_json,
        total_inflows: toFloat($total_inflows),
        total_outflows: toFloat($total_outflows),
        total_net_flow: toFloat($total_net_flow),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CASH_FLOW]->(x)
    RETURN x
    """
    params = {
        "id": summary.id,
        "user_id": user_id,
        "project_name": summary.project_name,
        "initial_investment": summary.initial_investment,
        "entries_json": json.dumps([e.model_dump() for e in summary.entries]),
        "total_inflows": summary.total_inflows,
        "total_outflows": summary.total_outflows,
        "total_net_flow": summary.total_net_flow,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return summary


async def list_summaries(session: AsyncSession, user_id: str) -> List[CashFlowSummary]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_FLOW]->(x:CashFlowSummary)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    return [_summary_from_node(dict(r["x"]), user_id) async for r in result]


async def get_summary(session: AsyncSession, user_id: str, cash_flow_id: str) -> Optional[CashFlowSummary]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_FLOW]->(x:CashFlowSummary {{id: $cash_flow_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, cash_flow_id=cash_flow_id)
    records = [r async for r in result]
    if not records:
        return None
    return _summary_from_node(dict(records[0]["x"]), user_id)
