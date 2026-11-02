"""
Basis Apportionment Service CRUD Operations

Rules and results are nodes, Book-gated, so one caller's basis-of-apportionment
register and apportionment history are invisible to another.

QUIRKS preserved:
* each basis endpoint reads its own key from the centre rows, and a centre without
  that key counts as zero, so it can still appear with a zero proportion;
* when the basis totals zero nothing is apportioned at all: the breakdown stays
  empty and the whole cost is reported as unapportioned;
* the apportioned total is accumulated row by row and the unapportioned figure is
  the cost less that accumulated total, so the two agree to floating point;
* the "cost" endpoint reads "asset_cost" on the rows, not "cost".
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from basis_apportionment_service.dependencies import book_id_var
from basis_apportionment_service.models import ApportionmentResult, ApportionmentRule
from neo4j import AsyncSession

RULE_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


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
        if iso in (None, "None"):
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _rule_from_node(n: Dict, user_id: str) -> ApportionmentRule:
    return ApportionmentRule(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        cost_type=n.get("cost_type") or "",
        basis=n.get("basis") or "",
        description=n.get("description") or "",
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _result_from_node(n: Dict, user_id: str) -> ApportionmentResult:
    raw = n.get("cost_centres_json")
    centres: List[Dict[str, Any]] = []
    if raw:
        try:
            centres = json.loads(raw)
        except (TypeError, ValueError):
            centres = []
    return ApportionmentResult(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        cost_type=n.get("cost_type") or "",
        total_cost=float(n.get("total_cost") or 0),
        basis=n.get("basis") or "",
        cost_centres=centres,
        total_apportioned=float(n.get("total_apportioned") or 0),
        unapportioned=float(n.get("unapportioned") or 0),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_rule(
    session: AsyncSession, user_id: str, *, cost_type: str, basis: str, description: str
) -> ApportionmentRule:
    rule = ApportionmentRule(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        cost_type=cost_type,
        basis=basis,
        description=description,
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:ApportionmentRule {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            cost_type: $cost_type,
            basis: $basis,
            description: $description,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_APPORTIONMENT_RULE]->(x)
        RETURN x
        """,
        {
            "id": rule.id,
            "user_id": user_id,
            "cost_type": cost_type,
            "basis": basis,
            "description": description,
            "created_at": rule.created_at.isoformat(),
        },
    )
    return rule


async def list_rules(session: AsyncSession, user_id: str) -> List[ApportionmentRule]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_APPORTIONMENT_RULE]->(x:ApportionmentRule)
    {RULE_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    return [_rule_from_node(dict(r["x"]), user_id) async for r in result]


async def apportion(
    session: AsyncSession,
    user_id: str,
    *,
    cost_type: str,
    total_cost: float,
    basis: str,
    basis_key: str,
    cost_centre_data: List[Dict[str, Any]],
) -> ApportionmentResult:
    result = ApportionmentResult(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        cost_type=cost_type,
        total_cost=total_cost,
        basis=basis,
    )

    basis_total = sum(centre.get(basis_key, 0) for centre in cost_centre_data)

    for centre in cost_centre_data:
        value = centre.get(basis_key, 0)
        if basis_total > 0:
            proportion = value / basis_total
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    basis_key: value,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned

    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:ApportionmentResult {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            cost_type: $cost_type,
            total_cost: toFloat($total_cost),
            basis: $basis,
            cost_centres_json: $cost_centres_json,
            total_apportioned: toFloat($total_apportioned),
            unapportioned: toFloat($unapportioned),
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_APPORTIONMENT_RESULT]->(x)
        RETURN x
        """,
        {
            "id": result.id,
            "user_id": user_id,
            "cost_type": cost_type,
            "total_cost": total_cost,
            "basis": basis,
            "cost_centres_json": json.dumps(result.cost_centres),
            "total_apportioned": result.total_apportioned,
            "unapportioned": result.unapportioned,
            "created_at": result.created_at.isoformat(),
        },
    )
    return result


async def list_results(
    session: AsyncSession, user_id: str, cost_type: Optional[str] = None
) -> List[ApportionmentResult]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_APPORTIONMENT_RESULT]->(x:ApportionmentResult)
    {RULE_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    results = [_result_from_node(dict(r["x"]), user_id) async for r in result]
    if cost_type:
        results = [r for r in results if r.cost_type == cost_type]
    return results
