"""CRUD operations for Expense Tracking Service.

Every query is scoped to the calling user and, when the request carries an
X-Book-ID, to that Book. Creates stamp book_id on the node; reads/writes
filter on it so cross-Book access is invisible (404).
"""

import uuid
from datetime import datetime, timezone
from typing import List, Optional

from expense_tracking_service.dependencies import book_id_var
from expense_tracking_service.exceptions import NotFoundError
from expense_tracking_service.models import Expense, ExpenseCreate, ExpenseStatus
from neo4j import AsyncSession


def _iso(value):
    """Coerce a Neo4j temporal (or test double) to an ISO string for Pydantic."""
    if value is None:
        return None
    if isinstance(value, str):
        return value
    if hasattr(value, "iso_format"):
        return value.iso_format()
    return str(value)


async def _run(session: AsyncSession, query: str, **params):
    merged = dict(params)
    merged["book_id"] = book_id_var.get()
    return await session.run(query, **merged)


async def create_expense(session: AsyncSession, user_id: str, expense: ExpenseCreate) -> Expense:
    expense_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc).isoformat()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (e:Expense {
        id: $id, user_id: $user_id, book_id: $book_id,
        company_id: $company_id, employee_id: $employee_id,
        category: $category, amount: toFloat($amount), currency: $currency,
        date: datetime($date), description: $description, vendor: $vendor,
        receipt_url: $receipt_url, project_code: $project_code,
        status: $status, approved_by: $approved_by,
        rejection_reason: $rejection_reason, created_at: datetime($created_at)
    })
    CREATE (u)-[:SUBMITTED]->(e)
    RETURN e
    """
    params = expense.model_dump()
    params.update(
        {
            "id": expense_id,
            "user_id": user_id,
            "status": ExpenseStatus.PENDING.value,
            "approved_by": "",
            "rejection_reason": "",
            "date": (expense.date or datetime.now(timezone.utc)).isoformat(),
            "created_at": now,
        }
    )
    result = await _run(session, query, **params)
    record = await result.single()
    if not record:
        raise NotFoundError(detail="Could not create expense", code="CREATE_FAILED")
    stored = expense.model_dump()
    stored.update(
        {
            "id": expense_id,
            "status": ExpenseStatus.PENDING.value,
            "approved_by": "",
            "rejection_reason": "",
            "created_at": datetime.fromisoformat(now),
        }
    )
    return Expense(**stored)


async def get_expenses(
    session: AsyncSession,
    user_id: str,
    company_id: str,
    category: Optional[str] = None,
    status_filter: Optional[str] = None,
    limit: int = 100,
) -> List[Expense]:
    filters = "e.company_id = $company_id"
    params = {"user_id": user_id, "company_id": company_id, "limit": limit}
    if category:
        filters += " AND e.category = $category"
        params["category"] = category
    if status_filter:
        filters += " AND e.status = $status_filter"
        params["status_filter"] = status_filter
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:SUBMITTED]->(e:Expense)
    WHERE ($book_id IS NULL OR e.book_id = $book_id)
      AND {filters}
    RETURN e
    ORDER BY e.created_at DESC
    LIMIT $limit
    """
    result = await _run(session, query, **params)
    expenses = []
    async for record in result:
        e = record["e"]
        expenses.append(
            Expense(
                id=e["id"],
                company_id=e["company_id"],
                employee_id=e["employee_id"],
                category=e["category"],
                amount=float(e["amount"]),
                currency=e["currency"],
                date=_iso(e.get("date")),
                description=e["description"],
                vendor=e["vendor"],
                receipt_url=e["receipt_url"],
                project_code=e["project_code"],
                status=e["status"],
                approved_by=e.get("approved_by", ""),
                rejection_reason=e.get("rejection_reason", ""),
                created_at=_iso(e.get("created_at")),
            )
        )
    return expenses


async def _set_expense_status(
    session: AsyncSession, user_id: str, expense_id: str, status: ExpenseStatus, **assignments
) -> Expense:
    set_parts = [f"e.status = '{status.value}'"]
    params = {"user_id": user_id, "expense_id": expense_id}
    for key, value in assignments.items():
        set_parts.append(f"e.{key} = ${key}")
        params[key] = value
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:SUBMITTED]->(e:Expense {{id: $expense_id}})
    WHERE ($book_id IS NULL OR e.book_id = $book_id)
    SET {', '.join(set_parts)}
    RETURN e
    """
    result = await _run(session, query, **params)
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"Expense {expense_id} not found", code="EXPENSE_NOT_FOUND")
    e = record["e"]
    return Expense(
        id=e["id"],
        company_id=e["company_id"],
        employee_id=e["employee_id"],
        category=e["category"],
        amount=float(e["amount"]),
        currency=e["currency"],
        date=_iso(e.get("date")),
        description=e["description"],
        vendor=e["vendor"],
        receipt_url=e["receipt_url"],
        project_code=e["project_code"],
        status=e["status"],
        approved_by=e.get("approved_by", ""),
        rejection_reason=e.get("rejection_reason", ""),
        created_at=_iso(e.get("created_at")),
    )


async def approve_expense(session: AsyncSession, user_id: str, expense_id: str, approver: str) -> Expense:
    return await _set_expense_status(session, user_id, expense_id, ExpenseStatus.APPROVED, approved_by=approver)


async def reject_expense(session: AsyncSession, user_id: str, expense_id: str, reason: str = "") -> Expense:
    return await _set_expense_status(session, user_id, expense_id, ExpenseStatus.REJECTED, rejection_reason=reason)


async def expense_summary(session: AsyncSession, user_id: str, company_id: str) -> dict:
    expenses = await get_expenses(session, user_id, company_id)
    by_category: dict = {}
    by_status: dict = {}
    total = 0.0
    for e in expenses:
        by_category[e.category.value] = by_category.get(e.category.value, 0.0) + e.amount
        by_status[e.status.value] = by_status.get(e.status.value, 0) + 1
        total += e.amount
    return {
        "company_id": company_id,
        "total_expenses": len(expenses),
        "total_amount": total,
        "by_category": by_category,
        "by_status": by_status,
    }
