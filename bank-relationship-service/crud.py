"""
Bank Relationship Service CRUD Operations

Neo4j-backed persistence for bank relationships and service quality
metrics. All records are stamped with book_id; every read applies the
Book filter `WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from bank_relationship_service.dependencies import book_id_var
from bank_relationship_service.exceptions import NotFoundError
from bank_relationship_service.models import (
    BankRelationship,
    BankRelationshipCreate,
    RelationshipStatus,
    ServiceQualityMetric,
    ServiceQualityMetricCreate,
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


def _relationship_from_node(n: Dict[str, Any], user_id: str) -> BankRelationship:
    return BankRelationship(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        bank_name=n["bank_name"],
        branch=n.get("branch", ""),
        account_number=n.get("account_number", ""),
        relationship_manager=n.get("relationship_manager", ""),
        contact_email=n.get("contact_email", ""),
        contact_phone=n.get("contact_phone", ""),
        services=json.loads(n.get("services_json") or "[]"),
        status=n.get("status", "active"),
        opened_date=_as_dt(n.get("opened_date")),
        rating=int(n.get("rating", 3)),
        notes=n.get("notes", ""),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _metric_from_node(n: Dict[str, Any], user_id: str) -> ServiceQualityMetric:
    return ServiceQualityMetric(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        relationship_id=n["relationship_id"],
        metric_name=n["metric_name"],
        score=int(n.get("score", 1)),
        notes=n.get("notes", ""),
        recorded_at=_as_dt(n.get("recorded_at")) or _now(),
    )


async def _get_relationship(session: AsyncSession, user_id: str, relationship_id: str) -> Optional[BankRelationship]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RELATIONSHIP]->(x:BankRelationship {{id: $relationship_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, relationship_id=relationship_id)
    records = [r async for r in result]
    if not records:
        return None
    return _relationship_from_node(dict(records[0]["x"]), user_id)


async def create_relationship(session: AsyncSession, user_id: str, payload: BankRelationshipCreate) -> BankRelationship:
    rel = BankRelationship(
        id=str(uuid.uuid4()),
        user_id=user_id,
        company_id=payload.company_id,
        bank_name=payload.bank_name,
        branch=payload.branch,
        account_number=payload.account_number,
        relationship_manager=payload.relationship_manager,
        contact_email=payload.contact_email,
        contact_phone=payload.contact_phone,
        services=list(payload.services),
        opened_date=payload.opened_date,
        rating=payload.rating,
        notes=payload.notes,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:BankRelationship {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        bank_name: $bank_name,
        branch: $branch,
        account_number: $account_number,
        relationship_manager: $relationship_manager,
        contact_email: $contact_email,
        contact_phone: $contact_phone,
        services_json: $services_json,
        status: $status,
        opened_date: datetime($opened_date),
        rating: toInteger($rating),
        notes: $notes,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_RELATIONSHIP]->(x)
    RETURN x
    """
    params = {
        "id": rel.id,
        "user_id": user_id,
        "company_id": rel.company_id,
        "bank_name": rel.bank_name,
        "branch": rel.branch,
        "account_number": rel.account_number,
        "relationship_manager": rel.relationship_manager,
        "contact_email": rel.contact_email,
        "contact_phone": rel.contact_phone,
        "services_json": json.dumps(list(rel.services)),
        "status": rel.status.value,
        "opened_date": rel.opened_date.isoformat() if rel.opened_date else None,
        "rating": rel.rating,
        "notes": rel.notes,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _relationship_from_node(dict(records[0]["x"]), user_id)


async def list_relationships(
    session: AsyncSession, user_id: str, company_id: str, status_filter: str = ""
) -> List[BankRelationship]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RELATIONSHIP]->(x:BankRelationship {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    rels = [_relationship_from_node(dict(r["x"]), user_id) async for r in result]
    if status_filter:
        rels = [r for r in rels if r.status.value == status_filter]
    return rels


async def update_relationship(
    session: AsyncSession,
    user_id: str,
    relationship_id: str,
    rating: Optional[int] = None,
    status: Optional[RelationshipStatus] = None,
    notes: Optional[str] = None,
) -> Dict[str, Any]:
    rel = await _get_relationship(session, user_id, relationship_id)
    if rel is None:
        raise NotFoundError("Relationship not found")
    if rating is not None:
        rel.rating = rating
    if status is not None:
        rel.status = status
    if notes is not None:
        rel.notes = notes
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RELATIONSHIP]->(x:BankRelationship {{id: $relationship_id}})
    {BOOK_FILTER}
    SET x.rating = toInteger($rating),
        x.status = $status,
        x.notes = $notes
    RETURN x
    """
    params = {
        "user_id": user_id,
        "relationship_id": relationship_id,
        "rating": rel.rating,
        "status": rel.status.value,
        "notes": rel.notes,
    }
    await _run(session, query, params)
    return {"id": relationship_id, "rating": rel.rating, "status": rel.status.value}


async def add_quality_metric(
    session: AsyncSession, user_id: str, payload: ServiceQualityMetricCreate
) -> Dict[str, Any]:
    # metric must attach to a relationship the caller owns and the Book can see
    rel = await _get_relationship(session, user_id, payload.relationship_id)
    if rel is None:
        raise NotFoundError("Relationship not found")
    metric = ServiceQualityMetric(
        relationship_id=rel.id,
        metric_name=payload.metric_name,
        score=payload.score,
        notes=payload.notes,
        recorded_at=_now(),
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ServiceQualityMetric {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        relationship_id: $relationship_id,
        metric_name: $metric_name,
        score: toInteger($score),
        notes: $notes,
        recorded_at: datetime($recorded_at),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_QUALITY_METRIC]->(x)
    RETURN x
    """
    params = {
        "id": metric.id,
        "user_id": user_id,
        "relationship_id": metric.relationship_id,
        "metric_name": metric.metric_name,
        "score": metric.score,
        "notes": metric.notes,
        "recorded_at": metric.recorded_at.isoformat(),
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return {"id": metric.id, "metric": metric.metric_name, "score": metric.score}


async def get_quality_metrics(session: AsyncSession, user_id: str, relationship_id: str) -> Dict[str, Any]:
    # only meaningful for relationships the caller owns and the Book can see
    rel = await _get_relationship(session, user_id, relationship_id)
    if rel is None:
        raise NotFoundError("Relationship not found")
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_QUALITY_METRIC]->(x:ServiceQualityMetric {{relationship_id: $relationship_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.recorded_at ASC
    """
    result = await _run(session, query, user_id=user_id, relationship_id=relationship_id)
    metrics = [_metric_from_node(dict(r["x"]), user_id) async for r in result]
    avg = sum(m.score for m in metrics) / len(metrics) if metrics else 0
    return {"relationship_id": relationship_id, "avg_score": avg, "metrics": metrics}


async def relationship_summary(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    rels = await list_relationships(session, user_id, company_id)
    active = sum(1 for r in rels if r.status == RelationshipStatus.ACTIVE)
    banks = len(set(r.bank_name for r in rels))
    services = set()
    for r in rels:
        services.update(r.services)
    avg_rating = sum(r.rating for r in rels) / max(1, len(rels))
    return {
        "company_id": company_id,
        "total_relationships": len(rels),
        "active": active,
        "unique_banks": banks,
        "total_services": len(services),
        "services": list(services),
        "avg_rating": avg_rating,
    }
