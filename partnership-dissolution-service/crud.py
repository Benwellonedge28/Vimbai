"""
Partnership Dissolution Service CRUD Operations

Dissolution reports persist as :DissolutionReport nodes, caller-owned
(X-User-Id) and Book-gated (X-Book-ID). Nested asset/creditor/partner
settlement lists and journal ids are stored as JSON props; the
realization/settlement math stays in the endpoint (pure computation).
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from partnership_dissolution_service.dependencies import book_id_var
from partnership_dissolution_service.models import DissolutionReason, DissolutionReport

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _as_dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _from_node(n: Dict) -> DissolutionReport:
    def _f(key: str, default: float = 0.0) -> float:
        v = n.get(key)
        return float(v) if v is not None else default

    return DissolutionReport(
        id=n["id"],
        partnership_id=n.get("partnership_id", ""),
        dissolution_date=_as_dt(n.get("dissolution_date")) or datetime.now(timezone.utc),
        reason=DissolutionReason(n.get("reason", "mutual_agreement")),
        total_assets_realized=_f("total_assets_realized"),
        total_liabilities_paid=_f("total_liabilities_paid"),
        total_creditors=_f("total_creditors"),
        total_partners_capitals=_f("total_partners_capitals"),
        realization_profit=_f("realization_profit"),
        realization_loss=_f("realization_loss"),
        assets=json.loads(n.get("assets") or "[]"),
        creditors=json.loads(n.get("creditors") or "[]"),
        partners=json.loads(n.get("partners") or "[]"),
        journal_entry_ids=json.loads(n.get("journal_entry_ids") or "[]"),
        status=n.get("status", "pending"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


def _params(r: DissolutionReport) -> Dict:
    return {
        "id": r.id,
        "partnership_id": r.partnership_id,
        "dissolution_date": r.dissolution_date.isoformat(),
        "reason": r.reason.value,
        "total_assets_realized": r.total_assets_realized,
        "total_liabilities_paid": r.total_liabilities_paid,
        "total_creditors": r.total_creditors,
        "total_partners_capitals": r.total_partners_capitals,
        "realization_profit": r.realization_profit,
        "realization_loss": r.realization_loss,
        "assets": json.dumps([a.model_dump(mode="json") for a in r.assets]),
        "creditors": json.dumps([c.model_dump(mode="json") for c in r.creditors]),
        "partners": json.dumps([p.model_dump(mode="json") for p in r.partners]),
        "journal_entry_ids": json.dumps(r.journal_entry_ids),
        "status": r.status,
        "created_at": r.created_at.isoformat(),
    }


async def create_dissolution(session: AsyncSession, user_id: str, report: DissolutionReport) -> DissolutionReport:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:DissolutionReport {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        partnership_id: $partnership_id,
        dissolution_date: datetime($dissolution_date),
        reason: $reason,
        total_assets_realized: toFloat($total_assets_realized),
        total_liabilities_paid: toFloat($total_liabilities_paid),
        total_creditors: toFloat($total_creditors),
        total_partners_capitals: toFloat($total_partners_capitals),
        realization_profit: toFloat($realization_profit),
        realization_loss: toFloat($realization_loss),
        assets: $assets,
        creditors: $creditors,
        partners: $partners,
        journal_entry_ids: $journal_entry_ids,
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DISSOLUTION]->(x)
    RETURN x
    """
    result = await _run(session, query, _params(report), user_id=user_id)
    records = [r async for r in result]
    return _from_node(dict(records[0]["x"]))


async def list_dissolutions(session: AsyncSession, user_id: str) -> List[DissolutionReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DISSOLUTION]->(x:DissolutionReport)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_from_node(dict(r["x"])) async for r in result]


async def get_dissolution(session: AsyncSession, user_id: str, dissolution_id: str) -> Optional[DissolutionReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DISSOLUTION]->(x:DissolutionReport)
    WHERE x.id = $dissolution_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, dissolution_id=dissolution_id, user_id=user_id)
    record = await result.single()
    return _from_node(dict(record["x"])) if record else None
