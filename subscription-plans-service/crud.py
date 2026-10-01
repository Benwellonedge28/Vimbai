"""
Subscription Plans Service CRUD Operations

Neo4j-backed persistence for plans and subscriptions. All records are
stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timedelta, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from subscription_plans_service.dependencies import book_id_var
from subscription_plans_service.exceptions import NotFoundError
from subscription_plans_service.models import BillingCycle, Plan, Subscription

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _plan_from_node(n: Dict, user_id: str) -> Plan:
    return Plan(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        tier=n.get("tier", "basic"),
        name=n["name"],
        price_monthly=float(n.get("price_monthly", 0)),
        features=json.loads(n.get("features_json") or "[]"),
        max_users=int(n.get("max_users", 5)),
        max_companies=int(n.get("max_companies", 1)),
        api_calls_per_month=int(n.get("api_calls_per_month", 1000)),
    )


def _sub_from_node(n: Dict, user_id: str) -> Subscription:
    return Subscription(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        plan_id=n["plan_id"],
        tier=n.get("tier", "basic"),
        billing_cycle=n.get("billing_cycle", "monthly"),
        start_date=n.get("start_date", ""),
        status=n.get("status", "active"),
        current_period_end=n.get("current_period_end", ""),
    )


async def create_plan(session: AsyncSession, user_id: str, plan: Plan) -> Plan:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Plan {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        tier: $tier,
        name: $name,
        price_monthly: toFloat($price_monthly),
        features_json: $features_json,
        max_users: toInteger($max_users),
        max_companies: toInteger($max_companies),
        api_calls_per_month: toInteger($api_calls_per_month),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PLAN]->(x)
    RETURN x
    """
    params = {
        "id": plan.id,
        "user_id": user_id,
        "tier": plan.tier.value,
        "name": plan.name,
        "price_monthly": plan.price_monthly,
        "features_json": json.dumps(list(plan.features)),
        "max_users": plan.max_users,
        "max_companies": plan.max_companies,
        "api_calls_per_month": plan.api_calls_per_month,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _plan_from_node(dict(records[0]["x"]), user_id)


async def list_plans(session: AsyncSession, user_id: str) -> List[Plan]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PLAN]->(x:Plan)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    return [_plan_from_node(dict(r["x"]), user_id) async for r in result]


async def get_plan(session: AsyncSession, user_id: str, plan_id: str) -> Optional[Plan]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PLAN]->(x:Plan {{id: $plan_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, plan_id=plan_id)
    records = [r async for r in result]
    if not records:
        return None
    return _plan_from_node(dict(records[0]["x"]), user_id)


async def create_subscription(
    session: AsyncSession,
    user_id: str,
    company_id: str,
    plan: Plan,
    cycle: BillingCycle,
) -> Subscription:
    sub_id = str(uuid.uuid4())
    days = {"monthly": 30, "quarterly": 90, "annual": 365}.get(cycle.value, 30)
    end = (_now() + timedelta(days=days)).strftime("%Y-%m-%d")
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Subscription {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        plan_id: $plan_id,
        tier: $tier,
        billing_cycle: $billing_cycle,
        start_date: $start_date,
        status: $status,
        current_period_end: $current_period_end,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_SUBSCRIPTION]->(x)
    RETURN x
    """
    params = {
        "id": sub_id,
        "user_id": user_id,
        "company_id": company_id,
        "plan_id": plan.id,
        "tier": plan.tier.value,
        "billing_cycle": cycle.value,
        "start_date": _now().strftime("%Y-%m-%d"),
        "status": "active",
        "current_period_end": end,
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _sub_from_node(dict(records[0]["x"]), user_id)


async def list_subscriptions(session: AsyncSession, user_id: str, company_id: str) -> List[Subscription]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUBSCRIPTION]->(x:Subscription {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_sub_from_node(dict(r["x"]), user_id) async for r in result]


async def get_subscription_by_plan(
    session: AsyncSession, user_id: str, company_id: str, plan_id: str
) -> Optional[Subscription]:
    for sub in await list_subscriptions(session, user_id, company_id):
        if sub.plan_id == plan_id:
            return sub
    return None
