"""
Partnership Agreement Service CRUD Operations

Agreements persist as :PartnershipAgreement nodes, caller-owned
(X-User-Id) and Book-gated (X-Book-ID), with partners stored as a JSON
prop. Updates follow the original setattr semantics over the hydrated
model and write back via params. Not-found lookups return None; the
caller keeps the original 404 responses.
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from partnership_agreement_service.dependencies import book_id_var
from partnership_agreement_service.models import Partner, PartnershipAgreement

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

_FLOAT_FIELDS = ("capital_amount", "max_drawings", "interest_on_capital_rate", "interest_on_drawings_rate")
_INT_FIELDS = ("duration_years",)
_BOOL_FIELDS = ("is_active", "drawings_allowed", "guaranteed_salary", "commission_allowed", "admission_new_partner")
_DT_FIELDS = ("start_date", "end_date", "created_at", "updated_at")
_STR_FIELDS = (
    "agreement_number",
    "partnership_name",
    "business_nature",
    "profit_sharing_basis",
    "retirement_conditions",
    "dissolution_conditions",
    "dispute_resolution",
    "agreement_document",
)


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


def _from_node(n: Dict) -> PartnershipAgreement:
    partners = [Partner(**p) for p in json.loads(n.get("partners") or "[]")]
    kwargs: Dict = {"id": n["id"], "partners": partners}
    for f in _STR_FIELDS:
        if f in n:
            kwargs[f] = n[f]
    for f in _FLOAT_FIELDS:
        if f in n and n[f] is not None:
            kwargs[f] = float(n[f])
    for f in _INT_FIELDS:
        if f in n and n[f] is not None:
            kwargs[f] = int(n[f])
    for f in _BOOL_FIELDS:
        if f in n:
            kwargs[f] = bool(n[f])
    for f in _DT_FIELDS:
        if f in n:
            dt = _as_dt(n[f])
            if dt is not None:
                kwargs[f] = dt
    return PartnershipAgreement(**kwargs)


def _props_clause() -> str:
    parts = ["id: $id", "user_id: $user_id", "book_id: $book_id"]
    parts += [f"{f}: ${f}" for f in _STR_FIELDS]
    parts += [f"{f}: toFloat(${f})" for f in _FLOAT_FIELDS]
    parts += [f"{f}: toInteger(${f})" for f in _INT_FIELDS]
    parts += [f"{f}: ${f}" for f in _BOOL_FIELDS]
    parts += [f"{f}: datetime(${f})" for f in _DT_FIELDS]
    parts.append("partners: $partners")
    return ",\n        ".join(parts)


def _set_clause() -> str:
    """Valid SET assignments (key = value), unlike CREATE's map syntax."""
    parts = []
    parts.append("x.id = $id")
    parts.append("x.user_id = $user_id")
    parts.append("x.book_id = $book_id")
    parts += [f"x.{f} = ${f}" for f in _STR_FIELDS]
    parts += [f"x.{f} = toFloat(${f})" for f in _FLOAT_FIELDS]
    parts += [f"x.{f} = toInteger(${f})" for f in _INT_FIELDS]
    parts += [f"x.{f} = ${f}" for f in _BOOL_FIELDS]
    parts += [f"x.{f} = datetime(${f})" for f in _DT_FIELDS]
    parts.append("x.partners = $partners")
    return ",\n        ".join(parts)


def _params(agg: PartnershipAgreement) -> Dict:
    params: Dict = {"id": agg.id}
    for f in _STR_FIELDS + _BOOL_FIELDS:
        params[f] = getattr(agg, f)
    for f in _FLOAT_FIELDS + _INT_FIELDS:
        params[f] = getattr(agg, f)
    for f in _DT_FIELDS:
        v = getattr(agg, f)
        params[f] = v.isoformat() if v else None
    params["partners"] = json.dumps([p.model_dump(mode="json") for p in agg.partners])
    return params


async def create_agreement(session: AsyncSession, user_id: str, agg: PartnershipAgreement) -> PartnershipAgreement:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:PartnershipAgreement {{
        {_props_clause()}
    }})
    CREATE (u)-[:OWNS_AGREEMENT]->(x)
    RETURN x
    """
    result = await _run(session, query, _params(agg), user_id=user_id)
    records = [r async for r in result]
    return _from_node(dict(records[0]["x"]))


async def list_agreements(session: AsyncSession, user_id: str) -> List[PartnershipAgreement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_AGREEMENT]->(x:PartnershipAgreement)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_from_node(dict(r["x"])) async for r in result]


async def get_agreement(session: AsyncSession, user_id: str, agreement_id: str) -> Optional[PartnershipAgreement]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_AGREEMENT]->(x:PartnershipAgreement)
    WHERE x.id = $agreement_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, agreement_id=agreement_id, user_id=user_id)
    record = await result.single()
    return _from_node(dict(record["x"])) if record else None


async def save_agreement(session: AsyncSession, user_id: str, agg: PartnershipAgreement) -> None:
    """Write back the full (mutated) agreement via params."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_AGREEMENT]->(x:PartnershipAgreement)
    WHERE x.id = $id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET {_set_clause()}
    """
    await _run(session, query, _params(agg), user_id=user_id)
