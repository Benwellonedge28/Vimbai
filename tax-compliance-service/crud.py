"""
Tax Compliance Service CRUD Operations

Tax obligations move from an in-memory _obligations dict to Neo4j:
:TaxObligation nodes via :OWNS_OBLIGATION edges, book_id stamped,
Book-gated reads. Summary aggregates (status counts, compliance score,
upcoming deadlines) are computed server-side in Python.
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from tax_compliance_service.dependencies import book_id_var
from tax_compliance_service.exceptions import NotFoundError
from tax_compliance_service.models import ComplianceStatus, ComplianceSummary, TaxObligation, TaxObligationCreate

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _obligation_from_node(n: Dict[str, Any]) -> TaxObligation:
    return TaxObligation(
        id=n["id"],
        company_id=n["company_id"],
        obligation_type=n["obligation_type"],
        description=n["description"],
        due_date=n["due_date"],
        amount=float(n.get("amount", 0)),
        status=ComplianceStatus(n.get("status", "pending")),
        filing_frequency=n.get("filing_frequency", "monthly"),
    )


async def create_obligation(session: AsyncSession, user_id: str, payload: TaxObligationCreate) -> TaxObligation:
    obligation = TaxObligation(id=str(uuid.uuid4()), **payload.model_dump())
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:TaxObligation {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        obligation_type: $obligation_type,
        description: $description,
        due_date: $due_date,
        amount: toFloat($amount),
        status: $status,
        filing_frequency: $filing_frequency,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_OBLIGATION]->(x)
    RETURN x
    """
    params = {
        "id": obligation.id,
        "user_id": user_id,
        "company_id": obligation.company_id,
        "obligation_type": obligation.obligation_type,
        "description": obligation.description,
        "due_date": obligation.due_date,
        "amount": obligation.amount,
        "status": obligation.status.value,
        "filing_frequency": obligation.filing_frequency,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return obligation


async def list_obligations(
    session: AsyncSession, user_id: str, company_id: str, status: str = ""
) -> List[TaxObligation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_OBLIGATION]->(x:TaxObligation {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.due_date ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    items = [_obligation_from_node(dict(r["x"])) async for r in result]
    if status:
        items = [o for o in items if o.status.value == status]
    return items


async def file_obligation(
    session: AsyncSession, user_id: str, company_id: str, obligation_id: str, filed_amount: float
) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_OBLIGATION]->(x:TaxObligation {{id: $obligation_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, obligation_id=obligation_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Obligation not found")
    node = dict(records[0]["x"])
    if node.get("company_id") != company_id:
        raise NotFoundError("Obligation not found")
    amount = float(filed_amount) if filed_amount else float(node.get("amount", 0))
    query = """
    MATCH (x:TaxObligation {id: $id})
    SET x.status = $status, x.amount = toFloat($amount)
    """
    await _run(session, query, id=obligation_id, status=ComplianceStatus.FILED.value, amount=amount)
    return {"filed": True, "obligation_id": obligation_id, "amount": amount}


async def get_summary(session: AsyncSession, user_id: str, company_id: str) -> ComplianceSummary:
    items = await list_obligations(session, user_id, company_id)
    now = _now()
    compliant = sum(1 for o in items if o.status == ComplianceStatus.COMPLIANT)
    pending = sum(1 for o in items if o.status == ComplianceStatus.PENDING)
    overdue = sum(1 for o in items if o.status == ComplianceStatus.OVERDUE)
    filed = sum(1 for o in items if o.status == ComplianceStatus.FILED)
    total = len(items)
    score = (compliant + filed) / total * 100 if total else 100

    upcoming = []
    for o in items:
        if o.status in (ComplianceStatus.PENDING, ComplianceStatus.OVERDUE):
            try:
                due = datetime.fromisoformat(o.due_date.replace("Z", "+00:00"))
                days_until = (due - now).days
                if days_until <= 30:
                    upcoming.append(
                        {
                            "obligation_id": o.id,
                            "type": o.obligation_type,
                            "due_date": o.due_date,
                            "days_until": days_until,
                            "amount": o.amount,
                            "status": o.status.value,
                        }
                    )
            except Exception:
                pass
    upcoming.sort(key=lambda x: x["days_until"])

    return ComplianceSummary(
        company_id=company_id,
        total_obligations=total,
        compliant=compliant,
        pending=pending,
        overdue=overdue,
        filed=filed,
        compliance_score=score,
        upcoming_deadlines=upcoming,
        obligations=items,
    )
