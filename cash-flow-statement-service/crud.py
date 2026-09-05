"""
Cash Flow Statement Service CRUD Operations

Neo4j-backed persistence for cash flow statements. All records are stamped
with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)` so unscoped (personal)
requests see their own data and Book-scoped requests see only their Book.

Nested activity line lists (operating/investing/financing) are stored as
JSON props. Ledger-style: statements are immutable records; corrections are
new statements (never in-place edits).
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from cash_flow_statement_service.dependencies import book_id_var
from cash_flow_statement_service.exceptions import NotFoundError
from cash_flow_statement_service.models import (
    CashFlowLine,
    CashFlowStatement,
    CashFlowStatementCreate,
    compute_totals,
)
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


def _dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    return datetime.fromisoformat(value.iso_format())


def _json(value) -> list:
    if value is None:
        return []
    if isinstance(value, str):
        return json.loads(value)
    return value


def _stmt_from_node(n: Dict[str, Any], user_id: str) -> CashFlowStatement:
    return CashFlowStatement(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        method=n.get("method", "direct"),
        period_start=_dt(n.get("period_start")),
        period_end=_dt(n.get("period_end")),
        operating_activities=[CashFlowLine(**l) for l in _json(n.get("operating_activities"))],
        investing_activities=[CashFlowLine(**l) for l in _json(n.get("investing_activities"))],
        financing_activities=[CashFlowLine(**l) for l in _json(n.get("financing_activities"))],
        net_operating=float(n["net_operating"]),
        net_investing=float(n["net_investing"]),
        net_financing=float(n["net_financing"]),
        net_change=float(n["net_change"]),
        beginning_cash=float(n["beginning_cash"]),
        ending_cash=float(n["ending_cash"]),
        created_at=_dt(n.get("created_at")),
    )


async def generate_statement(
    session: AsyncSession, user_id: str, payload: CashFlowStatementCreate
) -> CashFlowStatement:
    stmt = CashFlowStatement(
        company_id=payload.company_id,
        method=payload.method,
        period_start=payload.period_start,
        period_end=payload.period_end,
        beginning_cash=payload.beginning_cash,
        operating_activities=payload.operating_activities,
        investing_activities=payload.investing_activities,
        financing_activities=payload.financing_activities,
    )
    compute_totals(stmt)

    stmt_id = str(uuid.uuid4())
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:CashFlowStatement {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        method: $method,
        period_start: datetime($period_start),
        period_end: datetime($period_end),
        operating_activities: $operating,
        investing_activities: $investing,
        financing_activities: $financing,
        net_operating: toFloat($net_operating),
        net_investing: toFloat($net_investing),
        net_financing: toFloat($net_financing),
        net_change: toFloat($net_change),
        beginning_cash: toFloat($beginning_cash),
        ending_cash: toFloat($ending_cash),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_STATEMENT]->(x)
    RETURN x
    """
    params = {
        "id": stmt_id,
        "user_id": user_id,
        "company_id": stmt.company_id,
        "method": stmt.method.value,
        "period_start": (stmt.period_start or now).isoformat(),
        "period_end": (stmt.period_end or now).isoformat(),
        "operating": json.dumps([l.model_dump(mode="json") for l in stmt.operating_activities]),
        "investing": json.dumps([l.model_dump(mode="json") for l in stmt.investing_activities]),
        "financing": json.dumps([l.model_dump(mode="json") for l in stmt.financing_activities]),
        "net_operating": stmt.net_operating,
        "net_investing": stmt.net_investing,
        "net_financing": stmt.net_financing,
        "net_change": stmt.net_change,
        "beginning_cash": stmt.beginning_cash,
        "ending_cash": stmt.ending_cash,
        "created_at": now.isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _stmt_from_node(dict(records[0]["x"]), user_id)


async def get_history(session: AsyncSession, user_id: str, company_id: str) -> List[CashFlowStatement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_STATEMENT]->(x:CashFlowStatement {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_stmt_from_node(dict(r["x"]), user_id) async for r in result]


async def get_latest(session: AsyncSession, user_id: str, company_id: str) -> Optional[CashFlowStatement]:
    statements = await get_history(session, user_id, company_id)
    if not statements:
        return None
    return statements[-1]
