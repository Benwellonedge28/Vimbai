"""
Message Bus Service CRUD Operations

Published events and webhook subscriptions persist as Neo4j nodes,
caller-owned (X-User-Id) and Book-gated (X-Book-ID). Monitoring
endpoints (/events, /metrics, health counts) see only the caller's own
Book-visible records.
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from message_bus_service.dependencies import book_id_var
from message_bus_service.models import Event, EventDelivery, EventPriority, EventSubscription, EventType
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


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


def _iso(dt: Optional[datetime]) -> Optional[str]:
    if dt is None:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.isoformat()


def _j(v) -> str:
    return json.dumps(v if v is not None else {})


def _jd(s):
    if s in (None, ""):
        return {}
    if isinstance(s, dict):
        return s
    return json.loads(s)


# --- events ---


def _event_from_node(n: Dict) -> Event:
    return Event(
        id=n["id"],
        type=EventType(n.get("type", "")),
        source_service=n.get("source_service", ""),
        timestamp=_as_dt(n.get("timestamp")) or datetime.now(timezone.utc),
        priority=EventPriority(n.get("priority", "normal")),
        payload=_jd(n.get("payload")),
        correlation_id=n.get("correlation_id"),
        reply_to=n.get("reply_to"),
        headers=_jd(n.get("headers")) or None,
        retry_count=int(n.get("retry_count") or 0),
        max_retries=int(n.get("max_retries") or 3),
    )


async def create_event(session: AsyncSession, user_id: str, e: Event) -> Event:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:MessageBusEvent {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        type: $type,
        source_service: $source_service,
        timestamp: datetime($timestamp),
        priority: $priority,
        payload: $payload,
        correlation_id: $correlation_id,
        reply_to: $reply_to,
        headers: $headers,
        retry_count: toInteger($retry_count),
        max_retries: toInteger($max_retries)
    })
    CREATE (u)-[:OWNS_EVENT]->(x)
    RETURN x
    """
    params = {
        "id": e.id,
        "type": e.type.value,
        "source_service": e.source_service,
        "timestamp": _iso(e.timestamp),
        "priority": e.priority.value,
        "payload": _j(e.payload),
        "correlation_id": e.correlation_id,
        "reply_to": e.reply_to,
        "headers": _j(e.headers or {}),
        "retry_count": e.retry_count,
        "max_retries": e.max_retries,
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _event_from_node(dict(records[0]["x"]))


async def list_events(session: AsyncSession, user_id: str) -> List[Event]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_EVENT]->(x:MessageBusEvent)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_event_from_node(dict(r["x"])) async for r in result]


async def get_event(session: AsyncSession, user_id: str, event_id: str) -> Optional[Event]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_EVENT]->(x:MessageBusEvent)
    WHERE x.id = $event_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, event_id=event_id, user_id=user_id)
    record = await result.single()
    return _event_from_node(dict(record["x"])) if record else None


# --- subscriptions ---


def _sub_from_node(n: Dict) -> EventSubscription:
    event_types = [EventType(v) for v in json.loads(n.get("event_types") or "[]")]
    return EventSubscription(
        id=n["id"],
        name=n.get("name", ""),
        event_types=event_types,
        callback_url=n.get("callback_url", ""),
        filter_expression=n.get("filter_expression"),
        enabled=bool(n.get("enabled", True)),
        priority=EventPriority(n.get("priority", "normal")),
        secret=n.get("secret"),
    )


async def _delete_subscription_silent(session: AsyncSession, user_id: str, sub_id: str) -> None:
    """Remove the caller's subscription node if present (no-op otherwise)."""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUBSCRIPTION]->(x:MessageBusSubscription)
    WHERE x.id = $sub_id AND ($book_id IS NULL OR x.book_id = $book_id)
    DETACH DELETE x
    """
    await _run(session, query, sub_id=sub_id, user_id=user_id)


async def create_subscription(session: AsyncSession, user_id: str, s: EventSubscription) -> EventSubscription:
    # same client-supplied id overwrites the caller's prior subscription (dict-key semantics)
    await _delete_subscription_silent(session, user_id, s.id)
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:MessageBusSubscription {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        event_types: $event_types,
        callback_url: $callback_url,
        secret: $secret,
        filter_expression: $filter_expression,
        enabled: $enabled,
        priority: $priority
    })
    CREATE (u)-[:OWNS_SUBSCRIPTION]->(x)
    RETURN x
    """
    params = {
        "id": s.id,
        "name": s.name,
        "event_types": json.dumps([et.value for et in s.event_types]),
        "callback_url": s.callback_url,
        "secret": s.secret,
        "filter_expression": s.filter_expression,
        "enabled": s.enabled,
        "priority": s.priority.value,
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _sub_from_node(dict(records[0]["x"]))


async def list_subscriptions(session: AsyncSession, user_id: str) -> List[EventSubscription]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUBSCRIPTION]->(x:MessageBusSubscription)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_sub_from_node(dict(r["x"])) async for r in result]


async def get_subscription(session: AsyncSession, user_id: str, sub_id: str) -> Optional[EventSubscription]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUBSCRIPTION]->(x:MessageBusSubscription)
    WHERE x.id = $sub_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, sub_id=sub_id, user_id=user_id)
    record = await result.single()
    return _sub_from_node(dict(record["x"])) if record else None


async def delete_subscription(session: AsyncSession, user_id: str, sub_id: str) -> None:
    """Detach and remove the caller's subscription node (existence checked upstream)."""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUBSCRIPTION]->(x:MessageBusSubscription)
    WHERE x.id = $sub_id AND ($book_id IS NULL OR x.book_id = $book_id)
    DETACH DELETE x
    """
    await _run(session, query, sub_id=sub_id, user_id=user_id)


async def create_delivery(session: AsyncSession, user_id: str, d: EventDelivery) -> EventDelivery:
    """Persist one delivery attempt outcome (append-only audit trail)."""
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:MessageBusDelivery {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        event_id: $event_id,
        subscription_id: $subscription_id,
        callback_url: $callback_url,
        status: $status,
        response_status: toInteger($response_status),
        error: $error,
        timestamp: datetime($timestamp)
    })
    CREATE (u)-[:OWNS_DELIVERY]->(x)
    RETURN x
    """
    params = {
        "id": d.id,
        "event_id": d.event_id,
        "subscription_id": d.subscription_id,
        "callback_url": d.callback_url,
        "status": d.status,
        "response_status": d.response_status,
        "error": d.error,
        "timestamp": _iso(d.timestamp),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    n = dict(records[0]["x"])
    return EventDelivery(
        id=n["id"],
        event_id=n["event_id"],
        subscription_id=n.get("subscription_id"),
        callback_url=n["callback_url"],
        status=n["status"],
        response_status=n.get("response_status"),
        error=n.get("error"),
        timestamp=_as_dt(n.get("timestamp")),
    )


async def list_deliveries(session: AsyncSession, user_id: str, event_id: Optional[str] = None) -> List[EventDelivery]:
    """List the caller's Book-visible delivery records (newest first)."""
    extra = "AND x.event_id = $event_id" if event_id else ""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DELIVERY]->(x:MessageBusDelivery)
    {BOOK_FILTER}
    {extra}
    RETURN x
    """
    params = {"event_id": event_id} if event_id else {}
    result = await _run(session, query, params, user_id=user_id)
    deliveries = []
    async for r in result:
        n = dict(r["x"])
        deliveries.append(
            EventDelivery(
                id=n["id"],
                event_id=n["event_id"],
                subscription_id=n.get("subscription_id"),
                callback_url=n["callback_url"],
                status=n["status"],
                response_status=n.get("response_status"),
                error=n.get("error"),
                timestamp=_as_dt(n.get("timestamp")),
            )
        )
    return deliveries
