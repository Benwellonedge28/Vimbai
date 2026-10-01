"""
Zero-Based Budgeting Service CRUD Operations

Packages move from an in-memory _packages defaultdict to Neo4j:
:ZBBPackage nodes via :OWNS_PACKAGE edges, book_id stamped, Book-gated
reads. Items live on the package node as a JSON prop; total_amount is
recomputed in Python from the item list (the fake harness cannot do
node-prop arithmetic) and written back.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from zero_based_budgeting_service.dependencies import book_id_var
from zero_based_budgeting_service.exceptions import NotFoundError, ValidationError
from zero_based_budgeting_service.models import BudgetItem, ZBBPackage, ZBBPackageCreate, ZBBStatus

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _as_dt(value) -> datetime:
    if value is None:
        return _now()
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return _now()
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _pkg_from_node(n: Dict[str, Any], user_id: str) -> ZBBPackage:
    items = [BudgetItem(**i) for i in json.loads(n.get("items_json") or "[]")]
    return ZBBPackage(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        period=n["period"],
        name=n["name"],
        department=n["department"],
        items=items,
        total_amount=float(n.get("total_amount", 0)),
        status=ZBBStatus(n.get("status", "draft")),
        reviewer=n.get("reviewer", ""),
        review_notes=n.get("review_notes", ""),
        created_at=_as_dt(n.get("created_at")),
    )


async def _list_packages(
    session: AsyncSession, user_id: str, company_id: str, department: str = "", status_filter: str = ""
) -> List[ZBBPackage]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PACKAGE]->(x:ZBBPackage {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    pkgs = [_pkg_from_node(dict(r["x"]), user_id) async for r in result]
    if department:
        pkgs = [p for p in pkgs if p.department == department]
    if status_filter:
        pkgs = [p for p in pkgs if p.status.value == status_filter]
    return pkgs


async def create_package(session: AsyncSession, user_id: str, payload: ZBBPackageCreate) -> ZBBPackage:
    pkg = ZBBPackage(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=payload.company_id,
        period=payload.period,
        name=payload.name,
        department=payload.department,
        items=list(payload.items),
    )
    pkg.total_amount = sum(item.amount for item in pkg.items)
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ZBBPackage {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        period: $period,
        name: $name,
        department: $department,
        items_json: $items_json,
        total_amount: toFloat($total_amount),
        status: $status,
        reviewer: $reviewer,
        review_notes: $review_notes,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PACKAGE]->(x)
    RETURN x
    """
    params = {
        "id": pkg.id,
        "user_id": user_id,
        "company_id": pkg.company_id,
        "period": pkg.period,
        "name": pkg.name,
        "department": pkg.department,
        "items_json": json.dumps([i.model_dump() for i in pkg.items]),
        "total_amount": pkg.total_amount,
        "status": pkg.status.value,
        "reviewer": pkg.reviewer,
        "review_notes": pkg.review_notes,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return pkg


async def _get_package(session: AsyncSession, user_id: str, package_id: str) -> ZBBPackage:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PACKAGE]->(x:ZBBPackage {{id: $package_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, package_id=package_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Package not found")
    return _pkg_from_node(dict(records[0]["x"]), user_id)


async def _write_package(session: AsyncSession, pkg: ZBBPackage) -> None:
    query = """
    MATCH (x:ZBBPackage {id: $id})
    SET x.items_json = $items_json,
        x.total_amount = toFloat($total_amount),
        x.status = $status,
        x.reviewer = $reviewer,
        x.review_notes = $review_notes
    """
    params = {
        "id": pkg.id,
        "items_json": json.dumps([i.model_dump() for i in pkg.items]),
        "total_amount": pkg.total_amount,
        "status": pkg.status.value,
        "reviewer": pkg.reviewer,
        "review_notes": pkg.review_notes,
    }
    await _run(session, query, params)


async def get_packages(
    session: AsyncSession, user_id: str, company_id: str, department: str = "", status_filter: str = ""
) -> Dict[str, Any]:
    pkgs = await _list_packages(session, user_id, company_id, department, status_filter)
    return {"company_id": company_id, "packages": pkgs, "total": len(pkgs)}


async def add_item(session: AsyncSession, user_id: str, package_id: str, item: BudgetItem) -> Dict[str, Any]:
    pkg = await _get_package(session, user_id, package_id)
    pkg.items.append(item)
    pkg.total_amount = sum(i.amount for i in pkg.items)
    await _write_package(session, pkg)
    return {"package_id": package_id, "item_id": item.id, "total_amount": pkg.total_amount}


async def update_status(
    session: AsyncSession, user_id: str, package_id: str, status: ZBBStatus, reviewer: str = "", notes: str = ""
) -> Dict[str, Any]:
    pkg = await _get_package(session, user_id, package_id)
    pkg.status = status
    if reviewer:
        pkg.reviewer = reviewer
    if notes:
        pkg.review_notes = notes
    await _write_package(session, pkg)
    return {"id": package_id, "status": status.value}


async def set_item_priority(
    session: AsyncSession, user_id: str, item_id: str, priority: int, status: str = ""
) -> Dict[str, Any]:
    if priority < 1 or priority > 5:
        raise ValidationError("Priority must be 1-5")
    # items live as a JSON prop on the package: scan the caller's Book-visible
    # packages, locate the item in Python, write the package back
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PACKAGE]->(p:ZBBPackage)
    {BOOK_FILTER}
    RETURN p
    ORDER BY p.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    for r in [r async for r in result]:
        pkg = _pkg_from_node(dict(r["p"]), user_id)
        for item in pkg.items:
            if item.id == item_id:
                item.priority = priority
                if status:
                    item.status = status
                await _write_package(session, pkg)
                return {"item_id": item_id, "priority": item.priority, "status": item.status}
    raise NotFoundError("Item not found")


async def zbb_summary(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    pkgs = await _list_packages(session, user_id, company_id)
    if not pkgs:
        return {"company_id": company_id, "total_packages": 0, "total_budget": 0, "by_department": {}, "by_status": {}}
    total = sum(p.total_amount for p in pkgs)
    by_dept = {}
    by_status = {}
    for p in pkgs:
        by_dept[p.department] = by_dept.get(p.department, 0.0) + p.total_amount
        by_status[p.status.value] = by_status.get(p.status.value, 0) + 1
    return {
        "company_id": company_id,
        "total_packages": len(pkgs),
        "total_budget": total,
        "by_department": by_dept,
        "by_status": by_status,
    }
