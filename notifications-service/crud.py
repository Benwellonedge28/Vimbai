"""
Notifications Service CRUD Operations

Notification inbox records move from the in-memory
NotificationManager.user_notifications defaultdict to Neo4j:
one :VimbaiNotification node per recipient (each recipient owns
their own copy with their own read status) via :OWNS_NOTIFICATION
edges, stamped with the sender's Book context at send time.
Notification preferences move from a no-op PUT to
:NotificationPreference nodes via :OWNS_PREFERENCES (personal,
not Book-filtered - they are user settings).

Security fix carried by this move: previously any caller could
read ANY user's notifications by path id, mark anyone's
notifications read, delete them from anyone's inbox, and the
websocket mark_read handler scanned EVERY user's notifications
for the id. Reads and mutations are now caller-gated (the
recipient is always the caller) and Book-gated on the inbox
view; sending to arbitrary recipients remains the service's
job and is unchanged.
"""

import json
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from notifications_service import models
from notifications_service.dependencies import book_id_var

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _j(value: Any) -> str:
    return json.dumps(value, default=str)


def _load(name: str, n: Dict[str, Any], default: Any) -> Any:
    raw = n.get(name, default)
    if isinstance(raw, str):
        try:
            return json.loads(raw)
        except (ValueError, TypeError):
            return default
    return raw if raw is not None else default


def _coerce_dt(value: Any) -> Optional[datetime]:
    if isinstance(value, datetime):
        return value
    if value is None:
        return None
    if hasattr(value, "iso_format"):
        try:
            return datetime.fromisoformat(value.iso_format())
        except (TypeError, ValueError):
            return None
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return None
    return None


# ---------------------------------------------------------------------------
# Notification records (one node per recipient; owner = recipient)
# ---------------------------------------------------------------------------


def _notification_from_node(n: Dict[str, Any]) -> Any:
    return models.NotificationInDB(
        id=n["id"],
        type=n.get("type", models.NotificationType.SYSTEM.value),
        title=n["title"],
        message=n["message"],
        priority=n.get("priority", models.NotificationPriority.NORMAL.value),
        recipients=_load("recipients", n, []),
        channels=_load("channels", n, []),
        metadata=_load("metadata", n, None),
        action_url=n.get("action_url"),
        scheduled_at=_coerce_dt(n.get("scheduled_at")),
        expires_at=_coerce_dt(n.get("expires_at")),
        sender=n.get("sender"),
        status=n.get("status", "sent"),
        created_at=_coerce_dt(n.get("created_at")) or datetime.now(timezone.utc),
        sent_at=_coerce_dt(n.get("sent_at")),
        read_at=_coerce_dt(n.get("read_at")),
    )


async def create_notification(
    session: AsyncSession, recipient_id: str, notification: Any, book_id: Optional[str]
) -> None:
    """Persist one recipient's copy of a notification. book_id is the
    sender's Book context at send time (explicit: internal sends via the
    websocket path bind their own context)."""
    query = f"""
    MATCH (u:User {{id: $recipient_id}})
    CREATE (x:VimbaiNotification {{
        id: $id,
        user_id: $recipient_id,
        book_id: $book_id,
        type: $type,
        title: $title,
        message: $message,
        priority: $priority,
        recipients: $recipients,
        channels: $channels,
        metadata: $metadata,
        action_url: $action_url,
        scheduled_at: datetime($scheduled_at) ON CREATE NULL,
        expires_at: datetime($expires_at) ON CREATE NULL,
        sender: $sender,
        status: $status,
        created_at: datetime($created_at),
        sent_at: datetime($sent_at) ON CREATE NULL
    }})
    CREATE (u)-[:OWNS_NOTIFICATION]->(x)
    """
    await _run(
        session,
        query,
        recipient_id=recipient_id,
        user_id=recipient_id,  # the fake harness stamps edges from user_id
        book_id=book_id,
        id=notification.id,
        type=notification.type.value if hasattr(notification.type, "value") else str(notification.type),
        title=notification.title,
        message=notification.message,
        priority=notification.priority.value if hasattr(notification.priority, "value") else str(notification.priority),
        recipients=_j(notification.recipients),
        channels=_j([c.value if hasattr(c, "value") else str(c) for c in notification.channels]),
        metadata=_j(notification.metadata) if notification.metadata is not None else None,
        action_url=notification.action_url,
        scheduled_at=notification.scheduled_at.isoformat() if notification.scheduled_at else None,
        expires_at=notification.expires_at.isoformat() if notification.expires_at else None,
        sender=notification.sender,
        status=notification.status,
        created_at=notification.created_at.isoformat(),
        sent_at=notification.sent_at.isoformat() if notification.sent_at else None,
    )


async def list_notifications(
    session: AsyncSession, user_id: str, unread_only: bool = False, limit: int = 50
) -> List[Any]:
    """The caller's Book-visible inbox, newest first (original order)."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_NOTIFICATION]->(x:VimbaiNotification)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    notifications = [_notification_from_node(dict(r["x"])) async for r in result]
    if unread_only:
        notifications = [n for n in notifications if n.status != "read"]
    notifications.sort(key=lambda n: n.created_at, reverse=True)
    return notifications[:limit]


async def get_unread_count(session: AsyncSession, user_id: str) -> int:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_NOTIFICATION]->(x:VimbaiNotification)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    notifications = [_notification_from_node(dict(r["x"])) async for r in result]
    return sum(1 for n in notifications if n.status != "read")


async def find_notification(session: AsyncSession, user_id: str, notification_id: str) -> Optional[Any]:
    """Return the notification if it is in the caller's (Book-visible) inbox."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_NOTIFICATION]->(x:VimbaiNotification {{id: $notification_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, notification_id=notification_id)
    records = [r async for r in result]
    if not records:
        return None
    return _notification_from_node(dict(records[0]["x"]))


async def mark_notification_read(session: AsyncSession, user_id: str, notification_id: str) -> bool:
    query = f"""
    MATCH (x:VimbaiNotification {{id: $notification_id, user_id: $user_id}})
    {BOOK_FILTER}
    SET x.status = 'read', x.read_at = datetime($read_at)
    """
    await _run(
        session, query, notification_id=notification_id, user_id=user_id, read_at=datetime.now(timezone.utc).isoformat()
    )
    return True


async def mark_all_read(session: AsyncSession, user_id: str) -> int:
    """Mark the caller's Book-visible unread notifications read; returns count."""
    # (list_notifications already applies the Book filter; the per-node SET
    # below repeats it for production Cypher correctness.)
    unread = [n for n in await list_notifications(session, user_id) if n.status != "read"]
    read_at = datetime.now(timezone.utc).isoformat()
    for n in unread:
        query = f"""
        MATCH (x:VimbaiNotification {{id: $notification_id, user_id: $user_id}})
        {BOOK_FILTER}
        SET x.status = 'read', x.read_at = datetime($read_at)
        """
        await _run(session, query, notification_id=n.id, user_id=user_id, read_at=read_at)
    return len(unread)


async def delete_notification(session: AsyncSession, user_id: str, notification_id: str) -> bool:
    query = f"""
    MATCH (x:VimbaiNotification {{id: $notification_id, user_id: $user_id}})
    {BOOK_FILTER}
    DETACH DELETE x
    """
    await _run(session, query, notification_id=notification_id, user_id=user_id)
    return True


# ---------------------------------------------------------------------------
# Preferences (personal; caller-gated, not Book-filtered)
# ---------------------------------------------------------------------------


async def save_preferences(session: AsyncSession, user_id: str, preferences: Any) -> Any:
    """Upsert the caller's preference node (one per user)."""
    query = """
    MATCH (x:NotificationPreference {user_id: $user_id})
    DETACH DELETE x
    """
    await _run(session, query, user_id=user_id)
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:NotificationPreference {{
        user_id: $user_id,
        channels: $channels,
        quiet_hours_start: $quiet_hours_start,
        quiet_hours_end: $quiet_hours_end,
        email_batch: $email_batch,
        email_batch_interval_minutes: toInteger($email_batch_interval_minutes)
    }})
    CREATE (u)-[:OWNS_PREFERENCES]->(x)
    """
    channels = {
        (k.value if hasattr(k, "value") else str(k)): [c.value if hasattr(c, "value") else str(c) for c in v]
        for k, v in (preferences.channels or {}).items()
    }
    await _run(
        session,
        query,
        user_id=user_id,
        channels=_j(channels),
        quiet_hours_start=preferences.quiet_hours_start,
        quiet_hours_end=preferences.quiet_hours_end,
        email_batch=preferences.email_batch,
        email_batch_interval_minutes=preferences.email_batch_interval_minutes,
    )
    return preferences


async def get_preferences(session: AsyncSession, user_id: str) -> Optional[Any]:
    query = """
    MATCH (x:NotificationPreference {user_id: $user_id})
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    records = [r async for r in result]
    if not records:
        return None
    n = dict(records[0]["x"])
    return models.NotificationPreferences(
        user_id=n["user_id"],
        channels=_load("channels", n, {}),
        quiet_hours_start=n.get("quiet_hours_start"),
        quiet_hours_end=n.get("quiet_hours_end"),
        email_batch=bool(n.get("email_batch", True)),
        email_batch_interval_minutes=int(n.get("email_batch_interval_minutes", 60)),
    )
