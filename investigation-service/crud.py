"""
Investigation Service CRUD Operations

Neo4j-backed persistence for risk items. All records are stamped with
book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)` so unscoped (personal)
requests see their own data and Book-scoped requests see only their Book.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from investigation_service.dependencies import book_id_var
from investigation_service.exceptions import NotFoundError
from investigation_service.models import RiskCategory, RiskItem, RiskLevel, calc_level
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


def _item_from_node(n: Dict[str, Any], user_id: str) -> RiskItem:
    return RiskItem(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        category=RiskCategory(n["category"]),
        name=n["name"],
        description=n.get("description", ""),
        likelihood=int(n["likelihood"]),
        impact=int(n["impact"]),
        risk_score=float(n["risk_score"]),
        level=RiskLevel(n["level"]),
        owner=n.get("owner", ""),
        mitigation=n.get("mitigation", ""),
        status=n.get("status", "identified"),
        created_at=_dt(n.get("created_at")),
        updated_at=_dt(n.get("updated_at")),
    )


async def create_risk(session: AsyncSession, user_id: str, risk: RiskItem) -> RiskItem:
    risk_id = str(uuid.uuid4())
    now = _now()
    score = float(risk.likelihood * risk.impact)
    level = calc_level(score)

    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:RiskItem {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        category: $category,
        name: $name,
        description: $description,
        likelihood: toInteger($likelihood),
        impact: toInteger($impact),
        risk_score: toFloat($risk_score),
        level: $level,
        owner: $owner,
        mitigation: $mitigation,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    }})
    CREATE (u)-[:OWNS_RISK]->(x)
    RETURN x
    """
    params = {
        "id": risk_id,
        "user_id": user_id,
        "company_id": risk.company_id,
        "category": risk.category.value,
        "name": risk.name,
        "description": risk.description,
        "likelihood": risk.likelihood,
        "impact": risk.impact,
        "risk_score": score,
        "level": level.value,
        "owner": risk.owner,
        "mitigation": risk.mitigation,
        "status": "identified",
        "created_at": now.isoformat(),
        "updated_at": now.isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _item_from_node(dict(records[0]["x"]), user_id)


async def get_risks(
    session: AsyncSession,
    user_id: str,
    company_id: str,
    category: Optional[str] = None,
    level: Optional[str] = None,
) -> List[RiskItem]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RISK]->(x:RiskItem {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at DESC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    risks = [_item_from_node(dict(r["x"]), user_id) async for r in result]
    if category:
        risks = [r for r in risks if r.category.value == category]
    if level:
        risks = [r for r in risks if r.level.value == level]
    return risks


async def get_risk_by_id(session: AsyncSession, user_id: str, risk_id: str) -> Optional[RiskItem]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RISK]->(x:RiskItem {{id: $risk_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, risk_id=risk_id)
    records = [r async for r in result]
    if not records:
        return None
    return _item_from_node(dict(records[0]["x"]), user_id)


async def update_risk(
    session: AsyncSession,
    user_id: str,
    risk_id: str,
    likelihood: Optional[int] = None,
    impact: Optional[int] = None,
    mitigation: Optional[str] = None,
    status: Optional[str] = None,
) -> RiskItem:
    existing = await get_risk_by_id(session, user_id, risk_id)
    if existing is None:
        raise NotFoundError(detail="Risk not found")

    new_likelihood = likelihood if likelihood is not None else existing.likelihood
    new_impact = impact if impact is not None else existing.impact
    new_mitigation = mitigation if mitigation is not None else existing.mitigation
    new_status = status if status is not None else existing.status
    score = float(new_likelihood * new_impact)
    level = calc_level(score)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RISK]->(x:RiskItem {{id: $risk_id}})
    {BOOK_FILTER}
    SET x.likelihood = toInteger($likelihood),
        x.impact = toInteger($impact),
        x.risk_score = toFloat($risk_score),
        x.level = $level,
        x.mitigation = $mitigation,
        x.status = $status,
        x.updated_at = datetime($updated_at)
    RETURN x
    """
    params = {
        "risk_id": risk_id,
        "user_id": user_id,
        "likelihood": new_likelihood,
        "impact": new_impact,
        "risk_score": score,
        "level": level.value,
        "mitigation": new_mitigation,
        "status": new_status,
        "updated_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    if not records:
        raise NotFoundError(detail="Risk not found")
    return _item_from_node(dict(records[0]["x"]), user_id)


async def close_risk(session: AsyncSession, user_id: str, risk_id: str) -> RiskItem:
    existing = await get_risk_by_id(session, user_id, risk_id)
    if existing is None:
        raise NotFoundError(detail="Risk not found")

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RISK]->(x:RiskItem {{id: $risk_id}})
    {BOOK_FILTER}
    SET x.status = 'closed', x.updated_at = datetime($updated_at)
    RETURN x
    """
    result = await _run(session, query, risk_id=risk_id, user_id=user_id, updated_at=_now().isoformat())
    records = [r async for r in result]
    if not records:
        raise NotFoundError(detail="Risk not found")
    return _item_from_node(dict(records[0]["x"]), user_id)
