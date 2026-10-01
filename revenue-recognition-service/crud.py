"""
Revenue Recognition Service CRUD Operations

Neo4j-backed persistence for revenue contracts and their performance
obligations (IFRS 15 style allocation and recognition). All records are
stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from revenue_recognition_service.dependencies import book_id_var
from revenue_recognition_service.exceptions import NotFoundError
from revenue_recognition_service.models import (
    PerformanceObligation,
    RevenueContract,
    RevenueContractCreate,
)

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
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def allocate_price(contract: RevenueContract):
    """Allocate total transaction price across obligations by standalone selling price."""
    total_ssp = sum(o.standalone_selling_price for o in contract.obligations)
    if total_ssp > 0 and contract.total_transaction_price > 0:
        for o in contract.obligations:
            if o.standalone_selling_price > 0:
                o.transaction_price = contract.total_transaction_price * (o.standalone_selling_price / total_ssp)


def _contract_from_node(n: Dict[str, Any], user_id: str) -> RevenueContract:
    obligations = [PerformanceObligation(**o) for o in json.loads(n.get("obligations_json") or "[]")]
    return RevenueContract(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        customer_name=n["customer_name"],
        contract_date=_as_dt(n.get("contract_date")) or _now(),
        total_transaction_price=float(n.get("total_transaction_price", 0)),
        obligations=obligations,
        total_revenue_recognized=float(n.get("total_revenue_recognized", 0)),
        deferred_revenue=float(n.get("deferred_revenue", 0)),
        status=n.get("status", "active"),
    )


async def create_contract(session: AsyncSession, user_id: str, payload: RevenueContractCreate) -> RevenueContract:
    contract_id = str(uuid.uuid4())
    contract = RevenueContract(
        id=contract_id,
        user_id=user_id,
        company_id=payload.company_id,
        customer_name=payload.customer_name,
        contract_date=payload.contract_date or _now(),
        obligations=[
            PerformanceObligation(
                id=str(uuid.uuid4()),
                description=o.description,
                transaction_price=o.transaction_price,
                standalone_selling_price=o.standalone_selling_price,
                recognition_method=o.recognition_method,
            )
            for o in payload.obligations
        ],
    )
    contract.total_transaction_price = sum(o.transaction_price for o in contract.obligations)
    if any(o.standalone_selling_price > 0 for o in contract.obligations):
        allocate_price(contract)
    contract.deferred_revenue = contract.total_transaction_price - contract.total_revenue_recognized

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:RevenueContract {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        customer_name: $customer_name,
        contract_date: datetime($contract_date),
        total_transaction_price: toFloat($total_transaction_price),
        total_revenue_recognized: toFloat($total_revenue_recognized),
        deferred_revenue: toFloat($deferred_revenue),
        status: $status,
        obligations_json: $obligations_json,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CONTRACT]->(x)
    RETURN x
    """
    params = {
        "id": contract_id,
        "user_id": user_id,
        "company_id": contract.company_id,
        "customer_name": contract.customer_name,
        "contract_date": contract.contract_date.isoformat(),
        "total_transaction_price": contract.total_transaction_price,
        "total_revenue_recognized": contract.total_revenue_recognized,
        "deferred_revenue": contract.deferred_revenue,
        "status": contract.status,
        "obligations_json": json.dumps([o.model_dump(mode="json") for o in contract.obligations]),
        "created_at": _now().isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _contract_from_node(dict(records[0]["x"]), user_id)


async def get_contract(session: AsyncSession, user_id: str, contract_id: str) -> RevenueContract:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONTRACT]->(x:RevenueContract {{id: $contract_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, contract_id=contract_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Contract not found")
    return _contract_from_node(dict(records[0]["x"]), user_id)


async def list_contracts(session: AsyncSession, user_id: str, company_id: str) -> List[RevenueContract]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONTRACT]->(x:RevenueContract {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_contract_from_node(dict(r["x"]), user_id) async for r in result]


async def recognize_revenue(
    session: AsyncSession, user_id: str, contract_id: str, obligation_id: str, amount: float = 0
) -> Dict[str, Any]:
    contract = await get_contract(session, user_id, contract_id)
    for o in contract.obligations:
        if o.id == obligation_id:
            recog = amount if amount > 0 else o.transaction_price
            o.revenue_recognized = min(o.transaction_price, o.revenue_recognized + recog)
            o.is_satisfied = o.revenue_recognized >= o.transaction_price
            contract.total_revenue_recognized = sum(ob.revenue_recognized for ob in contract.obligations)
            contract.deferred_revenue = contract.total_transaction_price - contract.total_revenue_recognized
            await _write_back(session, user_id, contract)
            return {
                "obligation_id": obligation_id,
                "recognized": o.revenue_recognized,
                "is_satisfied": o.is_satisfied,
                "contract_total_recognized": contract.total_revenue_recognized,
                "deferred": contract.deferred_revenue,
            }
    raise NotFoundError("Contract or obligation not found")


async def _write_back(session: AsyncSession, user_id: str, contract: RevenueContract):
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONTRACT]->(x:RevenueContract {{id: $contract_id}})
    {BOOK_FILTER}
    SET x.obligations_json = $obligations_json,
        x.total_revenue_recognized = toFloat($total_revenue_recognized),
        x.deferred_revenue = toFloat($deferred_revenue)
    RETURN x
    """
    params = {
        "contract_id": contract.id,
        "obligations_json": json.dumps([o.model_dump(mode="json") for o in contract.obligations]),
        "total_revenue_recognized": contract.total_revenue_recognized,
        "deferred_revenue": contract.deferred_revenue,
    }
    await _run(session, query, params)


async def revenue_summary(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    contracts = await list_contracts(session, user_id, company_id)
    total = sum(c.total_transaction_price for c in contracts)
    recognized = sum(c.total_revenue_recognized for c in contracts)
    deferred = sum(c.deferred_revenue for c in contracts)
    return {
        "company_id": company_id,
        "total_contracts": len(contracts),
        "total_contract_value": total,
        "revenue_recognized": recognized,
        "deferred_revenue": deferred,
    }
