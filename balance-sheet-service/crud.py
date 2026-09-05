"""
Balance Sheet Service CRUD Operations

Neo4j-backed persistence for balance sheets. All records are stamped with
book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)` so unscoped (personal)
requests see their own data and Book-scoped requests see only their Book.

Nested item lists (assets/liabilities/equity) are stored as JSON props.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from balance_sheet_service.dependencies import book_id_var
from balance_sheet_service.exceptions import NotFoundError
from balance_sheet_service.models import (
    AssetItem,
    BalanceSheet,
    BalanceSheetCreate,
    EquityItem,
    LiabilityItem,
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


def _sheet_from_node(n: Dict[str, Any], user_id: str) -> BalanceSheet:
    return BalanceSheet(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        as_of_date=_dt(n.get("as_of_date")),
        assets=[AssetItem(**a) for a in _json(n.get("assets"))],
        liabilities=[LiabilityItem(**l) for l in _json(n.get("liabilities"))],
        equity=[EquityItem(**e) for e in _json(n.get("equity"))],
        total_assets=float(n["total_assets"]),
        total_liabilities=float(n["total_liabilities"]),
        total_equity=float(n["total_equity"]),
        is_balanced=bool(n["is_balanced"]),
        created_at=_dt(n.get("created_at")),
        updated_at=_dt(n.get("updated_at")),
    )


async def generate_sheet(session: AsyncSession, user_id: str, payload: BalanceSheetCreate) -> BalanceSheet:
    sheet = BalanceSheet(
        company_id=payload.company_id,
        as_of_date=payload.as_of_date or _now(),
        assets=payload.assets,
        liabilities=payload.liabilities,
        equity=payload.equity,
    )
    compute_totals(sheet)

    sheet_id = str(uuid.uuid4())
    now = _now()
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:BalanceSheet {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        as_of_date: datetime($as_of_date),
        assets: $assets,
        liabilities: $liabilities,
        equity: $equity,
        total_assets: toFloat($total_assets),
        total_liabilities: toFloat($total_liabilities),
        total_equity: toFloat($total_equity),
        is_balanced: $is_balanced,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    }})
    CREATE (u)-[:OWNS_SHEET]->(x)
    RETURN x
    """
    params = {
        "id": sheet_id,
        "user_id": user_id,
        "company_id": sheet.company_id,
        "as_of_date": sheet.as_of_date.isoformat(),
        "assets": json.dumps([a.model_dump(mode="json") for a in sheet.assets]),
        "liabilities": json.dumps([l.model_dump(mode="json") for l in sheet.liabilities]),
        "equity": json.dumps([e.model_dump(mode="json") for e in sheet.equity]),
        "total_assets": sheet.total_assets,
        "total_liabilities": sheet.total_liabilities,
        "total_equity": sheet.total_equity,
        "is_balanced": sheet.is_balanced,
        "created_at": now.isoformat(),
        "updated_at": now.isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _sheet_from_node(dict(records[0]["x"]), user_id)


async def get_history(session: AsyncSession, user_id: str, company_id: str) -> List[BalanceSheet]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SHEET]->(x:BalanceSheet {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_sheet_from_node(dict(r["x"]), user_id) async for r in result]


async def get_latest(session: AsyncSession, user_id: str, company_id: str) -> Optional[BalanceSheet]:
    sheets = await get_history(session, user_id, company_id)
    if not sheets:
        return None
    return sheets[-1]
