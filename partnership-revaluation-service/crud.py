"""
Partnership Revaluation Service CRUD Operations

Revaluation reports persist as :RevaluationReport nodes, caller-owned
(X-User-Id) and Book-gated (X-Book-ID). Entries and ratio dicts are
stored as JSON props.
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from partnership_revaluation_service.dependencies import book_id_var
from partnership_revaluation_service.models import GoodwillTreatment, RevaluationReport

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


def _from_node(n: Dict) -> RevaluationReport:
    def _f(key: str, default: float = 0.0) -> float:
        v = n.get(key)
        return float(v) if v is not None else default

    return RevaluationReport(
        id=n["id"],
        partnership_id=n.get("partnership_id", ""),
        revaluation_date=_as_dt(n.get("revaluation_date")) or datetime.now(timezone.utc),
        entries=json.loads(n.get("entries") or "[]"),
        total_increase=_f("total_increase"),
        total_decrease=_f("total_decrease"),
        net_gain=_f("net_gain"),
        goodwill_amount=_f("goodwill_amount"),
        goodwill_treatment=GoodwillTreatment(n.get("goodwill_treatment", "raise_and_raise")),
        new_profit_sharing_ratios=json.loads(n.get("new_profit_sharing_ratios") or "{}"),
        journal_entry_ids=json.loads(n.get("journal_entry_ids") or "[]"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


def _params(r: RevaluationReport) -> Dict:
    return {
        "id": r.id,
        "partnership_id": r.partnership_id,
        "revaluation_date": r.revaluation_date.isoformat(),
        "entries": json.dumps([e.model_dump(mode="json") for e in r.entries]),
        "total_increase": r.total_increase,
        "total_decrease": r.total_decrease,
        "net_gain": r.net_gain,
        "goodwill_amount": r.goodwill_amount,
        "goodwill_treatment": r.goodwill_treatment.value,
        "new_profit_sharing_ratios": json.dumps(r.new_profit_sharing_ratios),
        "journal_entry_ids": json.dumps(r.journal_entry_ids),
        "created_at": r.created_at.isoformat(),
    }


async def create_revaluation(session: AsyncSession, user_id: str, report: RevaluationReport) -> RevaluationReport:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:RevaluationReport {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        partnership_id: $partnership_id,
        revaluation_date: datetime($revaluation_date),
        entries: $entries,
        total_increase: toFloat($total_increase),
        total_decrease: toFloat($total_decrease),
        net_gain: toFloat($net_gain),
        goodwill_amount: toFloat($goodwill_amount),
        goodwill_treatment: $goodwill_treatment,
        new_profit_sharing_ratios: $new_profit_sharing_ratios,
        journal_entry_ids: $journal_entry_ids,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_REVALUATION]->(x)
    RETURN x
    """
    result = await _run(session, query, _params(report), user_id=user_id)
    records = [r async for r in result]
    return _from_node(dict(records[0]["x"]))


async def list_revaluations(session: AsyncSession, user_id: str) -> List[RevaluationReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REVALUATION]->(x:RevaluationReport)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_from_node(dict(r["x"])) async for r in result]


async def get_revaluation(session: AsyncSession, user_id: str, revaluation_id: str) -> Optional[RevaluationReport]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REVALUATION]->(x:RevaluationReport)
    WHERE x.id = $revaluation_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, revaluation_id=revaluation_id, user_id=user_id)
    record = await result.single()
    return _from_node(dict(record["x"])) if record else None
