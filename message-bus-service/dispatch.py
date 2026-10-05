"""Autonomous event dispatch for the Vimbai Book event bus.

Every published event is delivered, inside the same Book context:

1. Webhook fan-out - the caller's enabled subscriptions that listen for
   the event type receive an HMAC-signed POST at their callback URL.
   Every attempt is recorded as an immutable :OWNS_DELIVERY audit node.

2. Built-in Book automations - transaction and journal-entry change
   events automatically re-evaluate the caller's alert rules, so alerts
   fire without anyone asking. The reactions run with the same actor
   identity and Book context as the original write (X-User-Id /
   X-Book-ID, injected by the API gateway).

Delivery failures never fail the publish: the event stays on the bus,
the failure is audited, and the Book keeps working.
"""

import hashlib
import hmac
import os
from datetime import datetime

import httpx
from message_bus_service import crud
from message_bus_service.dependencies import book_id_var
from message_bus_service.models import Event, EventDelivery, EventType

# How the bus reaches sibling services inside the cluster (overridable
# per environment via env vars; service-to-service calls bypass the
# gateway and carry the gateway-injected identity headers directly).
ALERTS_SERVICE_URL = os.getenv("ALERTS_SERVICE_URL", "http://localhost:8090")

WEBHOOK_TIMEOUT_SECONDS = 5.0
AUTOMATION_TIMEOUT_SECONDS = 5.0

# Change events that automatically re-evaluate the caller's alert rules.
ALERT_EVALUATION_EVENTS = {
    EventType.JOURNAL_ENTRY_CREATED,
    EventType.JOURNAL_ENTRY_UPDATED,
    EventType.JOURNAL_ENTRY_DELETED,
    EventType.TRANSACTION_CREATED,
    EventType.TRANSACTION_UPDATED,
    EventType.TRANSACTION_DELETED,
    EventType.BOOK_RESOURCE_CREATED,
    EventType.BOOK_RESOURCE_UPDATED,
    EventType.BOOK_RESOURCE_DELETED,
    EventType.BANK_FEED_RECEIVED,
    EventType.BUDGET_VARIANCE_ALERT,
}


def _signature(secret: str, body: str) -> str:
    return hmac.new(secret.encode(), body.encode(), hashlib.sha256).hexdigest()


async def _deliver_webhook(subscription, event: Event) -> EventDelivery:
    """POST the event to a subscriber's callback URL (HMAC-signed)."""
    body = event.model_dump_json(exclude={"retry_count", "max_retries"})
    headers = {"Content-Type": "application/json", "X-Vimbai-Event": event.type.value}
    if subscription.secret:
        headers["X-Vimbai-Signature"] = _signature(subscription.secret, body)
    try:
        async with httpx.AsyncClient(timeout=WEBHOOK_TIMEOUT_SECONDS) as client:
            resp = await client.post(subscription.callback_url, content=body, headers=headers)
        return EventDelivery(
            event_id=event.id,
            subscription_id=subscription.id,
            callback_url=subscription.callback_url,
            status="delivered" if resp.status_code < 400 else "failed",
            response_status=resp.status_code,
        )
    except Exception as exc:  # noqa: BLE001 - any transport error is audited, never raised
        return EventDelivery(
            event_id=event.id,
            subscription_id=subscription.id,
            callback_url=subscription.callback_url,
            status="failed",
            error=str(exc)[:500],
        )


async def _run_alert_evaluation(user_id: str, event: Event) -> None:
    """Built-in automation: re-evaluate the caller's alert rules."""
    headers = {
        "Content-Type": "application/json",
        "X-User-Id": user_id,
        "X-Book-ID": event.payload.get("book_id", "") or (book_id_var.get() or ""),
    }
    data = {
        "event_type": event.type.value,
        "source_service": event.source_service,
        "timestamp": event.timestamp.isoformat(),
        **event.payload,
    }
    try:
        async with httpx.AsyncClient(timeout=AUTOMATION_TIMEOUT_SECONDS) as client:
            await client.post(f"{ALERTS_SERVICE_URL}/evaluate", json=data, headers=headers)
    except Exception:  # noqa: BLE001 - automations must never break the bus
        # Alerts stay available on their own /evaluate endpoint; a failed
        # automation run is a missed reaction, not a data loss.
        pass


async def dispatch_event(session, user_id: str, event: Event) -> list:
    """Deliver one event inside its Book context.

    Returns the list of delivery records created (possibly empty).
    Any failure is swallowed and audited - publishing always succeeds.
    """
    deliveries = []
    try:
        subscriptions = await crud.list_subscriptions(session, user_id)
        matching = [s for s in subscriptions if s.enabled and event.type in s.event_types]
        for sub in matching:
            delivery = await _deliver_webhook(sub, event)
            deliveries.append(delivery)
            await crud.create_delivery(session, user_id, delivery)

        if event.type in ALERT_EVALUATION_EVENTS:
            await _run_alert_evaluation(user_id, event)
    except Exception:  # noqa: BLE001 - dispatch must never break publish
        pass
    return deliveries


def dispatch_summary(deliveries: list) -> dict:
    """Small metric block for publish responses."""
    return {
        "delivered": sum(1 for d in deliveries if d.status == "delivered"),
        "failed": sum(1 for d in deliveries if d.status != "delivered"),
        "timestamp": datetime.utcnow().isoformat(),
    }
