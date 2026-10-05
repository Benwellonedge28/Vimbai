"""
Vimbai Message Bus Service
RabbitMQ-based event bus for asynchronous communication between microservices

Published events and webhook subscriptions persist in Neo4j, stamped
with the caller (X-User-Id) and the Book context (X-Book-ID, verified
upstream by the API gateway). Monitoring endpoints see only the
caller's own Book-visible records. The in-memory 10000-event ring
buffer is replaced by durable storage; RabbitMQ transport behavior is
unchanged (fallback mode when RabbitMQ is unreachable).

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import asyncio
import hashlib
import importlib.util
import json
import os as _os
import sys as _sys
from datetime import datetime
from typing import Any, Callable, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "message_bus_service" not in _sys.modules or not hasattr(_sys.modules.get("message_bus_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("message_bus_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["message_bus_service"] = _pkg
    _sys.modules["message_bus_service"].__path__ = [_HERE]

import aio_pika
from aio_pika import DeliveryMode, ExchangeType, Message
from dotenv import load_dotenv
from fastapi import BackgroundTasks, Depends, FastAPI, HTTPException, Request
from message_bus_service import crud
from message_bus_service.database import Neo4jConnector
from message_bus_service.dependencies import book_id_var, get_db_session, get_user_id
from message_bus_service.dispatch import dispatch_event, dispatch_summary
from message_bus_service.exceptions import MessageBusServiceError
from message_bus_service.models import (
    DEAD_LETTER_EXCHANGE,
    EXCHANGE_NAME,
    Event,
    EventPriority,
    EventSubscription,
    EventType,
    QueueConfig,
)
from neo4j import AsyncSession
from pydantic import BaseModel, Field

load_dotenv()

# ============================================================================
# Configuration
# ============================================================================

RABBITMQ_URL = _os.getenv("RABBITMQ_URL", "amqp://guest:guest@localhost:5672/")

app = FastAPI(
    title="Vimbai Message Bus Service",
    description="RabbitMQ-based event bus for async microservices communication",
    version="1.0.0",
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(MessageBusServiceError)
async def _mb_error(request: Request, exc: MessageBusServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


# ============================================================================
# Configuration
# ============================================================================

RABBITMQ_URL = _os.getenv("RABBITMQ_URL", "amqp://guest:guest@localhost:5672/")

# ============================================================================
# Enums and Models
# ============================================================================


class EventSubscription(BaseModel):
    id: str
    name: str
    event_types: List[EventType]
    callback_url: str
    filter_expression: Optional[str] = None
    enabled: bool = True
    priority: EventPriority = EventPriority.NORMAL
    secret: Optional[str] = None


class QueueConfig(BaseModel):
    name: str
    durable: bool = True
    auto_delete: bool = False
    max_length: Optional[int] = None
    message_ttl: Optional[int] = None
    dead_letter_exchange: str = DEAD_LETTER_EXCHANGE


# ============================================================================
# Event Bus Core
# ============================================================================


class EventBus:
    """Main event bus for publishing and subscribing to events"""

    def __init__(self):
        self.subscribers: Dict[EventType, List[Callable]] = {}
        self.rabbitmq_connection = None
        self.rabbitmq_channel = None

    async def connect(self):
        """Connect to RabbitMQ"""
        try:
            self.rabbitmq_connection = await aio_pika.connect_robust(RABBITMQ_URL)
            self.rabbitmq_channel = await self.rabbitmq_connection.channel()

            # Declare main exchange
            await self.rabbitmq_channel.declare_exchange(EXCHANGE_NAME, ExchangeType.TOPIC, durable=True)

            # Declare dead letter exchange
            await self.rabbitmq_channel.declare_exchange(DEAD_LETTER_EXCHANGE, ExchangeType.TOPIC, durable=True)

            # Declare main queue
            main_queue = await self.rabbitmq_channel.declare_queue(
                "vimbai_events",
                durable=True,
                arguments={
                    "x-dead-letter-exchange": DEAD_LETTER_EXCHANGE,
                    "x-message-ttl": 86400000,  # 24 hours
                },
            )

            # Bind queue to exchange with wildcard routing
            await main_queue.bind(EXCHANGE_NAME, routing_key="#")

            print("[EventBus] Connected to RabbitMQ")
            return True

        except Exception as e:
            print(f"[EventBus] Failed to connect to RabbitMQ: {e}")
            print("[EventBus] Running in fallback mode (in-memory only)")
            return False

    async def disconnect(self):
        """Disconnect from RabbitMQ"""
        if self.rabbitmq_connection:
            await self.rabbitmq_connection.close()

    async def publish(self, event: Event) -> bool:
        """Publish an event to the message bus"""
        try:
            # Try to publish to RabbitMQ
            if self.rabbitmq_channel:
                exchange = await self.rabbitmq_channel.get_exchange(EXCHANGE_NAME)

                message_body = json.dumps(
                    {
                        "id": event.id,
                        "type": event.type.value,
                        "source_service": event.source_service,
                        "timestamp": event.timestamp.isoformat(),
                        "priority": event.priority.value,
                        "payload": event.payload,
                        "correlation_id": event.correlation_id,
                        "retry_count": event.retry_count,
                    }
                )

                message = Message(
                    body=message_body.encode(),
                    delivery_mode=DeliveryMode.PERSISTENT,
                    content_type="application/json",
                    headers=event.headers or {},
                    correlation_id=event.correlation_id or "",
                )

                await exchange.publish(message, routing_key=event.type.value)

                print(f"[EventBus] Published event: {event.type.value}")
                return True
            else:
                # Fallback: just store in memory
                print(f"[EventBus] Fallback publish (no RabbitMQ): {event.type.value}")
                return True

        except Exception as e:
            print(f"[EventBus] Failed to publish event: {e}")
            return False

    async def subscribe(self, event_type: EventType, callback: Callable):
        """Subscribe to an event type"""
        if event_type not in self.subscribers:
            self.subscribers[event_type] = []
        self.subscribers[event_type].append(callback)

    async def handle_message(self, message: aio_pika.IncomingMessage):
        """Handle incoming message"""
        async with message.process():
            try:
                data = json.loads(message.body.decode())
                event = Event(
                    id=data.get("id", ""),
                    type=EventType(data.get("type", "")),
                    source_service=data.get("source_service", ""),
                    timestamp=datetime.fromisoformat(data.get("timestamp", datetime.utcnow().isoformat())),
                    priority=EventPriority(data.get("priority", "normal")),
                    payload=data.get("payload", {}),
                    correlation_id=data.get("correlation_id"),
                    retry_count=data.get("retry_count", 0),
                )

                # Call registered callbacks
                if event.type in self.subscribers:
                    for callback in self.subscribers[event.type]:
                        try:
                            await callback(event)
                        except Exception as e:
                            print(f"[EventBus] Callback error: {e}")

            except Exception as e:
                print(f"[EventBus] Failed to handle message: {e}")


# Global event bus instance
event_bus = EventBus()


# ============================================================================
# Helper Functions
# ============================================================================


def create_event(
    event_type: EventType,
    source_service: str,
    payload: Dict[str, Any],
    priority: EventPriority = EventPriority.NORMAL,
    correlation_id: Optional[str] = None,
) -> Event:
    """Create a new event"""
    return Event(
        type=event_type,
        source_service=source_service,
        payload=payload,
        priority=priority,
        correlation_id=correlation_id,
    )


# ============================================================================
# API Endpoints
# ============================================================================


@app.on_event("startup")
async def startup():
    await event_bus.connect()


@app.on_event("shutdown")
async def shutdown():
    await event_bus.disconnect()


@app.get("/")
async def health_check(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return {
        "status": "healthy",
        "service": "message-bus",
        "connected": event_bus.rabbitmq_connection is not None,
        "events_stored": len(await crud.list_events(db_session, user_id)),
        "subscriptions": len(await crud.list_subscriptions(db_session, user_id)),
    }


# --- Event Publishing ---


@app.post("/events/publish")
async def publish_event(
    event: Event,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Publish an event to the message bus"""
    success = await event_bus.publish(event)
    if success:
        await crud.create_event(db_session, user_id, event)
        # Autonomous reactions: webhook fan-out + built-in Book
        # automations (alert-rule re-evaluation on transaction changes).
        deliveries = await dispatch_event(db_session, user_id, event)

    if success:
        return {
            "status": "published",
            "event_id": event.id,
            "event_type": event.type.value,
            "dispatch": dispatch_summary(deliveries),
        }
    else:
        raise HTTPException(status_code=500, detail="Failed to publish event")


@app.post("/events/{event_type}/publish")
async def publish_event_by_type(
    event_type: EventType,
    source_service: str,
    payload: Dict[str, Any],
    priority: EventPriority = EventPriority.NORMAL,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Publish an event by type (convenience endpoint)"""
    event = create_event(
        event_type=event_type,
        source_service=source_service,
        payload=payload,
        priority=priority,
    )

    success = await event_bus.publish(event)
    deliveries = []
    if success:
        await crud.create_event(db_session, user_id, event)
        # Autonomous reactions: webhook fan-out + built-in Book
        # automations (alert-rule re-evaluation on transaction changes).
        deliveries = await dispatch_event(db_session, user_id, event)

    return {
        "status": "published" if success else "failed",
        "event_id": event.id,
        "event_type": event_type.value,
        "dispatch": dispatch_summary(deliveries),
    }


# --- Event Subscription ---


@app.post("/subscriptions", status_code=201)
async def create_subscription(
    subscription: EventSubscription,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a webhook subscription for events"""
    return await crud.create_subscription(db_session, user_id, subscription)


@app.get("/subscriptions")
async def list_subscriptions(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all event subscriptions"""
    return await crud.list_subscriptions(db_session, user_id)


@app.get("/subscriptions/{subscription_id}")
async def get_subscription(
    subscription_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific subscription"""
    sub = await crud.get_subscription(db_session, user_id, subscription_id)
    if not sub:
        raise HTTPException(status_code=404, detail="Subscription not found")
    return sub


@app.delete("/subscriptions/{subscription_id}")
async def delete_subscription(
    subscription_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a subscription"""
    sub = await crud.get_subscription(db_session, user_id, subscription_id)
    if not sub:
        raise HTTPException(status_code=404, detail="Subscription not found")
    await crud.delete_subscription(db_session, user_id, subscription_id)
    return {"status": "deleted"}


# --- Event Store ---


@app.get("/events")
async def list_events(
    event_type: Optional[EventType] = None,
    source_service: Optional[str] = None,
    limit: int = 100,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List recent events"""
    filtered = await crud.list_events(db_session, user_id)

    if event_type:
        filtered = [e for e in filtered if e.type == event_type]
    if source_service:
        filtered = [e for e in filtered if e.source_service == source_service]

    # Sort by timestamp descending
    filtered.sort(key=lambda x: x.timestamp, reverse=True)

    return filtered[:limit]


@app.get("/events/{event_id}/deliveries")
async def list_event_deliveries(
    event_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delivery attempts for one of the caller's events (audit trail)."""
    event = await crud.get_event(db_session, user_id, event_id)
    if not event:
        raise HTTPException(status_code=404, detail="Event not found")
    return await crud.list_deliveries(db_session, user_id, event_id)


@app.get("/events/{event_id}")
async def get_event(
    event_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific event"""
    event = await crud.get_event(db_session, user_id, event_id)
    if not event:
        raise HTTPException(status_code=404, detail="Event not found")
    return event


# --- Event Types ---


@app.get("/event-types")
async def list_event_types():
    """List all available event types"""
    return [{"name": et.name, "value": et.value} for et in EventType]


# --- Metrics ---


@app.get("/metrics")
async def get_metrics(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get message bus metrics"""
    event_counts = {}
    for event in await crud.list_events(db_session, user_id):
        event_type = event.type.value
        event_counts[event_type] = event_counts.get(event_type, 0) + 1

    return {
        "total_events": len(await crud.list_events(db_session, user_id)),
        "event_counts_by_type": event_counts,
        "total_subscriptions": len(await crud.list_subscriptions(db_session, user_id)),
        "rabbitmq_connected": event_bus.rabbitmq_connection is not None,
    }


# --- Specific Event Triggers (for testing) ---


@app.post("/trigger/journal-entry-created")
async def trigger_journal_entry_created(
    entry_id: str,
    amount: float,
    description: str = "Test journal entry",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Trigger a journal entry created event (for testing)"""
    event = create_event(
        event_type=EventType.JOURNAL_ENTRY_CREATED,
        source_service="accounting-service",
        payload={
            "entry_id": entry_id,
            "amount": amount,
            "description": description,
        },
    )
    await event_bus.publish(event)
    await crud.create_event(db_session, user_id, event)
    await dispatch_event(db_session, user_id, event)
    return {"status": "triggered", "event_id": event.id}


@app.post("/trigger/budget-variance-alert")
async def trigger_budget_variance_alert(
    budget_id: str,
    variance_percent: float,
    category: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Trigger a budget variance alert"""
    priority = EventPriority.HIGH if variance_percent > 20 else EventPriority.NORMAL

    event = create_event(
        event_type=EventType.BUDGET_VARIANCE_ALERT,
        source_service="finance-service",
        payload={
            "budget_id": budget_id,
            "variance_percent": variance_percent,
            "category": category,
        },
        priority=priority,
    )
    await event_bus.publish(event)
    await crud.create_event(db_session, user_id, event)
    await dispatch_event(db_session, user_id, event)
    return {"status": "triggered", "event_id": event.id}


@app.post("/trigger/transaction-flagged")
async def trigger_transaction_flagged(
    transaction_id: str,
    fraud_score: float,
    reason: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Trigger a transaction flagged event"""
    priority = EventPriority.CRITICAL if fraud_score > 0.8 else EventPriority.HIGH

    event = create_event(
        event_type=EventType.TRANSACTION_FLAGGED,
        source_service="fraud-detection-service",
        payload={
            "transaction_id": transaction_id,
            "fraud_score": fraud_score,
            "reason": reason,
        },
        priority=priority,
    )
    await event_bus.publish(event)
    await crud.create_event(db_session, user_id, event)
    await dispatch_event(db_session, user_id, event)
    return {"status": "triggered", "event_id": event.id}


@app.post("/trigger/approval-requested")
async def trigger_approval_requested(
    approval_id: str,
    requester: str,
    approvers: List[str],
    amount: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Trigger an approval requested event"""
    event = create_event(
        event_type=EventType.APPROVAL_REQUESTED,
        source_service="workflow-service",
        payload={
            "approval_id": approval_id,
            "requester": requester,
            "approvers": approvers,
            "amount": amount,
        },
        priority=EventPriority.HIGH,
    )
    await event_bus.publish(event)
    await crud.create_event(db_session, user_id, event)
    await dispatch_event(db_session, user_id, event)
    return {"status": "triggered", "event_id": event.id}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8097)
