"""
Appropriation Control Service CRUD Operations

Neo4j-backed persistence for departmental appropriations and their
control transactions. All records are stamped with book_id; every read
applies the Book filter `WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from appropriation_control_service.dependencies import book_id_var
from appropriation_control_service.exceptions import NotFoundError
from appropriation_control_service.models import (
    Appropriation,
    AppropriationCreate,
    AppropriationTransaction,
    AppropriationTransactionCreate,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

TX_TYPES = ("commit", "spend", "uncommit", "refund")


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _appr_from_node(n: Dict[str, Any], user_id: str) -> Appropriation:
    return Appropriation(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        department=n["department"],
        fiscal_year=n.get("fiscal_year", ""),
        approved_amount=float(n.get("approved_amount", 0)),
        spent_amount=float(n.get("spent_amount", 0)),
        committed_amount=float(n.get("committed_amount", 0)),
        available_amount=float(n.get("available_amount", 0)),
        status=n.get("status", "active"),
    )


async def _get_appropriation(session: AsyncSession, user_id: str, appropriation_id: str) -> Optional[Appropriation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_APPROPRIATION]->(x:Appropriation {{id: $appropriation_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, appropriation_id=appropriation_id)
    records = [r async for r in result]
    if not records:
        return None
    return _appr_from_node(dict(records[0]["x"]), user_id)


async def _write_back(session: AsyncSession, appr: Appropriation):
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_APPROPRIATION]->(x:Appropriation {{id: $id}})
    {BOOK_FILTER}
    SET x.spent_amount = toFloat($spent_amount),
        x.committed_amount = toFloat($committed_amount),
        x.available_amount = toFloat($available_amount),
        x.status = $status
    RETURN x
    """
    params = {
        "user_id": appr.user_id,
        "id": appr.id,
        "spent_amount": appr.spent_amount,
        "committed_amount": appr.committed_amount,
        "available_amount": appr.available_amount,
        "status": appr.status,
    }
    await _run(session, query, params)


async def create_appropriation(session: AsyncSession, user_id: str, payload: AppropriationCreate) -> Appropriation:
    appr = Appropriation(
        id=str(uuid.uuid4()),
        user_id=user_id,
        company_id=payload.company_id,
        department=payload.department,
        fiscal_year=payload.fiscal_year,
        approved_amount=payload.approved_amount,
        spent_amount=payload.spent_amount,
        committed_amount=payload.committed_amount,
    )
    appr.available_amount = appr.approved_amount - appr.committed_amount - appr.spent_amount
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Appropriation {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        department: $department,
        fiscal_year: $fiscal_year,
        approved_amount: toFloat($approved_amount),
        spent_amount: toFloat($spent_amount),
        committed_amount: toFloat($committed_amount),
        available_amount: toFloat($available_amount),
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_APPROPRIATION]->(x)
    RETURN x
    """
    params = {
        "id": appr.id,
        "user_id": user_id,
        "company_id": appr.company_id,
        "department": appr.department,
        "fiscal_year": appr.fiscal_year,
        "approved_amount": appr.approved_amount,
        "spent_amount": appr.spent_amount,
        "committed_amount": appr.committed_amount,
        "available_amount": appr.available_amount,
        "status": appr.status,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _appr_from_node(dict(records[0]["x"]), user_id)


async def list_appropriations(
    session: AsyncSession, user_id: str, company_id: str, department: str = ""
) -> List[Appropriation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_APPROPRIATION]->(x:Appropriation {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    apprs = [_appr_from_node(dict(r["x"]), user_id) async for r in result]
    if department:
        apprs = [a for a in apprs if a.department == department]
    return apprs


async def create_transaction(
    session: AsyncSession,
    user_id: str,
    payload: AppropriationTransactionCreate,
) -> Dict[str, Any]:
    if payload.type not in TX_TYPES:
        raise NotFoundError(f"Unknown transaction type: {payload.type}")
    appr = await _get_appropriation(session, user_id, payload.appropriation_id)
    if appr is None:
        raise NotFoundError("Appropriation not found")

    tx = AppropriationTransaction(
        appropriation_id=appr.id,
        type=payload.type,
        amount=payload.amount,
        description=payload.description,
        date=_now(),
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AppropriationTransaction {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        appropriation_id: $appropriation_id,
        type: $type,
        amount: toFloat($amount),
        description: $description,
        date: datetime($date),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_APPROPRIATION_TX]->(x)
    RETURN x
    """
    params = {
        "id": tx.id,
        "user_id": user_id,
        "appropriation_id": tx.appropriation_id,
        "type": tx.type,
        "amount": tx.amount,
        "description": tx.description,
        "date": tx.date.isoformat(),
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)

    # Apply the control semantics and write the aggregates back to the node.
    if tx.type == "commit":
        appr.committed_amount += tx.amount
    elif tx.type == "spend":
        appr.spent_amount += tx.amount
        appr.committed_amount -= tx.amount
    elif tx.type == "uncommit":
        appr.committed_amount -= tx.amount
    elif tx.type == "refund":
        appr.spent_amount -= tx.amount
    appr.available_amount = appr.approved_amount - appr.committed_amount - appr.spent_amount
    if appr.available_amount <= 0:
        appr.status = "exhausted"
    await _write_back(session, appr)
    return {"id": tx.id, "available": appr.available_amount, "status": appr.status}


async def check_available(session: AsyncSession, user_id: str, appropriation_id: str, amount: float) -> Dict[str, Any]:
    appr = await _get_appropriation(session, user_id, appropriation_id)
    if appr is None:
        raise NotFoundError("Appropriation not found")
    can_spend = appr.available_amount >= amount
    return {
        "appropriation_id": appropriation_id,
        "available": appr.available_amount,
        "requested": amount,
        "allowed": can_spend,
    }
