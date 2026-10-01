"""
Webhook Service CRUD Operations

Endpoints move from an in-memory _endpoints defaultdict to
:WebhookEndpoint nodes via :OWNS_ENDPOINT edges; deliveries move from
_deliveries to :WebhookDelivery nodes via :OWNS_DELIVERY edges with the
endpoint_id stamped on the node. The events list is stored as a JSON
prop. Every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from webhook_service.dependencies import book_id_var
from webhook_service.exceptions import NotFoundError
from webhook_service.models import WebhookDelivery, WebhookEndpoint, WebhookEndpointCreate

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


def _endpoint_from_node(n: Dict[str, Any], user_id: str) -> WebhookEndpoint:
    return WebhookEndpoint(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        url=n["url"],
        secret=n.get("secret", ""),
        events=json.loads(n.get("events_json") or "[]"),
        active=bool(n.get("active", True)),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _delivery_from_node(n: Dict[str, Any], user_id: str) -> WebhookDelivery:
    return WebhookDelivery(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        endpoint_id=n["endpoint_id"],
        event_type=n["event_type"],
        payload=json.loads(n.get("payload_json") or "{}"),
        status=n.get("status", "pending"),
        attempts=int(n.get("attempts", 0)),
        response_code=int(n.get("response_code", 0)),
        last_attempt=_as_dt(n.get("last_attempt")),
    )


async def create_endpoint(session: AsyncSession, user_id: str, payload: WebhookEndpointCreate) -> WebhookEndpoint:
    endpoint = WebhookEndpoint(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=payload.company_id,
        url=payload.url,
        secret=payload.secret,
        events=list(payload.events),
        active=payload.active,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:WebhookEndpoint {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        url: $url,
        secret: $secret,
        events_json: $events_json,
        active: $active,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ENDPOINT]->(x)
    RETURN x
    """
    params = {
        "id": endpoint.id,
        "user_id": user_id,
        "company_id": endpoint.company_id,
        "url": endpoint.url,
        "secret": endpoint.secret,
        "events_json": json.dumps(endpoint.events),
        "active": endpoint.active,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)
    return endpoint


async def get_endpoints(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ENDPOINT]->(x:WebhookEndpoint {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    endpoints = [_endpoint_from_node(dict(r["x"]), user_id) async for r in result]
    return {"company_id": company_id, "endpoints": endpoints, "total": len(endpoints)}


async def get_active_endpoints_for_event(
    session: AsyncSession, user_id: str, company_id: str, event_type: str
) -> List[WebhookEndpoint]:
    """Caller's Book-visible endpoints subscribed to the event (active, and
    either subscribed to all events (empty list) or to this one)."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ENDPOINT]->(x:WebhookEndpoint {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    endpoints = [_endpoint_from_node(dict(r["x"]), user_id) async for r in result]
    return [e for e in endpoints if e.active and (not e.events or event_type in e.events)]


async def store_delivery(session: AsyncSession, user_id: str, delivery: WebhookDelivery) -> None:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:WebhookDelivery {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        endpoint_id: $endpoint_id,
        event_type: $event_type,
        payload_json: $payload_json,
        status: $status,
        attempts: toInteger($attempts),
        response_code: toInteger($response_code),
        last_attempt: datetime($last_attempt)
    })
    CREATE (u)-[:OWNS_DELIVERY]->(x)
    RETURN x
    """
    params = {
        "id": delivery.id,
        "user_id": user_id,
        "endpoint_id": delivery.endpoint_id,
        "event_type": delivery.event_type,
        "payload_json": json.dumps(delivery.payload),
        "status": delivery.status,
        "attempts": delivery.attempts,
        "response_code": delivery.response_code,
        "last_attempt": (delivery.last_attempt or _now()).isoformat(),
    }
    await _run(session, query, params)


async def get_deliveries(session: AsyncSession, user_id: str, endpoint_id: str, limit: int = 50) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DELIVERY]->(x:WebhookDelivery {{endpoint_id: $endpoint_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, endpoint_id=endpoint_id)
    deliveries = [_delivery_from_node(dict(r["x"]), user_id) async for r in result]
    tail = deliveries[-limit:] if limit else deliveries
    return {"endpoint_id": endpoint_id, "deliveries": tail, "total": len(deliveries)}
