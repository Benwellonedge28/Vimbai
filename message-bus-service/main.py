"""
Vimbai Message Bus Service
RabbitMQ-based event bus for asynchronous communication between microservices
"""

import asyncio
import hashlib
import json
import os
from datetime import datetime
from enum import Enum
from typing import Any, Callable, Dict, List, Optional

import aio_pika
from aio_pika import DeliveryMode, ExchangeType, Message
from dotenv import load_dotenv
from fastapi import BackgroundTasks, Depends, FastAPI, HTTPException
from pydantic import BaseModel, Field

load_dotenv()

app = FastAPI(
    title="Vimbai Message Bus Service",
    description="RabbitMQ-based event bus for async microservices communication",
    version="1.0.0",
)

# ============================================================================
# Configuration
# ============================================================================

RABBITMQ_URL = os.getenv("RABBITMQ_URL", "amqp://guest:guest@localhost:5672/")
EXCHANGE_NAME = "vimbai_events"
DEAD_LETTER_EXCHANGE = "vimbai_dlx"

# ============================================================================
# Enums and Models
# ============================================================================


class EventType(str, Enum):
    # Accounting Events
    JOURNAL_ENTRY_CREATED = "accounting.journal_entry.created"
    JOURNAL_ENTRY_UPDATED = "accounting.journal_entry.updated"
    JOURNAL_ENTRY_POSTED = "accounting.journal_entry.posted"
    ACCOUNT_CREATED = "accounting.account.created"
    ACCOUNT_UPDATED = "accounting.account.updated"
    TRIAL_BALANCE_GENERATED = "accounting.trial_balance.generated"

    # Finance Events
    BUDGET_CREATED = "finance.budget.created"
    BUDGET_UPDATED = "finance.budget.updated"
    BUDGET_VARIANCE_ALERT = "finance.budget.variance_alert"
    SCENARIO_CREATED = "finance.scenario.created"

    # Transaction Events
    TRANSACTION_CREATED = "banking.transaction.created"
    TRANSACTION_RECONCILED = "banking.transaction.reconciled"
    TRANSACTION_FLAGGED = "fraud.transaction.flagged"

    # Workflow Events
    APPROVAL_REQUESTED = "workflow.approval.requested"
    APPROVAL_COMPLETED = "workflow.approval.completed"
    APPROVAL_REJECTED = "workflow.approval.rejected"

    # Integration Events
    POS_SYNC_COMPLETED = "integration.pos.sync_completed"
    BANK_FEED_RECEIVED = "integration.bank_feed.received"
    INVENTORY_UPDATED = "integration.inventory.updated"

    # Multimodal Events
    DOCUMENT_PROCESSED = "multimodal.document.processed"
    VOICE_TRANSCRIPT_COMPLETE = "multimodal.voice.transcript_complete"

    # System Events
    SERVICE_HEALTHY = "system.service.healthy"
    SERVICE_UNHEALTHY = "system.service.unhealthy"
    FEATURE_TOGGLED = "system.feature.toggled"
    DATA_SYNC_COMPLETED = "system.data_sync.completed"


class EventPriority(str, Enum):
    LOW = "low"
    NORMAL = "normal"
    HIGH = "high"
    CRITICAL = "critical"


class Event(BaseModel):
    id: str = Field(default_factory=lambda: hashlib.md5(str(datetime.utcnow()).encode()).hexdigest()[:16])
    type: EventType
    source_service: str
    timestamp: datetime = Field(default_factory=datetime.utcnow)
    priority: EventPriority = EventPriority.NORMAL
    payload: Dict[str, Any]
    correlation_id: Optional[str] = None
    reply_to: Optional[str] = None
    headers: Optional[Dict[str, str]] = None
    retry_count: int = 0
    max_retries: int = 3


class EventSubscription(BaseModel):
    id: str
    name: str
    event_types: List[EventType]
    callback_url: str
    filter_expression: Optional[str] = None
    enabled: bool = True
    priority: EventPriority = EventPriority.NORMAL


class QueueConfig(BaseModel):
    name: str
    durable: bool = True
    auto_delete: bool = False
    max_length: Optional[int] = None
    message_ttl: Optional[int] = None
    dead_letter_exchange: str = DEAD_LETTER_EXCHANGE


# ============================================================================
# In-Memory Event Store (for monitoring and debugging)
# ============================================================================

event_store: List[Event] = []
subscriptions: Dict[str, EventSubscription] = {}
connection = None
channel = None

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
            # Store in memory for monitoring
            event_store.append(event)
            if len(event_store) > 10000:
                event_store.pop(0)  # Keep last 10000 events

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
async def health_check():
    return {
        "status": "healthy",
        "service": "message-bus",
        "connected": event_bus.rabbitmq_connection is not None,
        "events_stored": len(event_store),
        "subscriptions": len(subscriptions),
    }


# --- Event Publishing ---


@app.post("/events/publish")
async def publish_event(event: Event):
    """Publish an event to the message bus"""
    success = await event_bus.publish(event)

    if success:
        return {"status": "published", "event_id": event.id, "event_type": event.type.value}
    else:
        raise HTTPException(status_code=500, detail="Failed to publish event")


@app.post("/events/{event_type}/publish")
async def publish_event_by_type(
    event_type: EventType,
    source_service: str,
    payload: Dict[str, Any],
    priority: EventPriority = EventPriority.NORMAL,
):
    """Publish an event by type (convenience endpoint)"""
    event = create_event(
        event_type=event_type,
        source_service=source_service,
        payload=payload,
        priority=priority,
    )

    success = await event_bus.publish(event)

    return {"status": "published" if success else "failed", "event_id": event.id, "event_type": event_type.value}


# --- Event Subscription ---


@app.post("/subscriptions", status_code=201)
async def create_subscription(subscription: EventSubscription):
    """Create a webhook subscription for events"""
    subscriptions[subscription.id] = subscription
    return subscription


@app.get("/subscriptions")
async def list_subscriptions():
    """List all event subscriptions"""
    return list(subscriptions.values())


@app.get("/subscriptions/{subscription_id}")
async def get_subscription(subscription_id: str):
    """Get a specific subscription"""
    if subscription_id not in subscriptions:
        raise HTTPException(status_code=404, detail="Subscription not found")
    return subscriptions[subscription_id]


@app.delete("/subscriptions/{subscription_id}")
async def delete_subscription(subscription_id: str):
    """Delete a subscription"""
    if subscription_id in subscriptions:
        del subscriptions[subscription_id]
        return {"status": "deleted"}
    raise HTTPException(status_code=404, detail="Subscription not found")


# --- Event Store ---


@app.get("/events")
async def list_events(event_type: Optional[EventType] = None, source_service: Optional[str] = None, limit: int = 100):
    """List recent events"""
    filtered = event_store

    if event_type:
        filtered = [e for e in filtered if e.type == event_type]
    if source_service:
        filtered = [e for e in filtered if e.source_service == source_service]

    # Sort by timestamp descending
    filtered.sort(key=lambda x: x.timestamp, reverse=True)

    return filtered[:limit]


@app.get("/events/{event_id}")
async def get_event(event_id: str):
    """Get a specific event"""
    for event in event_store:
        if event.id == event_id:
            return event
    raise HTTPException(status_code=404, detail="Event not found")


# --- Event Types ---


@app.get("/event-types")
async def list_event_types():
    """List all available event types"""
    return [{"name": et.name, "value": et.value} for et in EventType]


# --- Metrics ---


@app.get("/metrics")
async def get_metrics():
    """Get message bus metrics"""
    event_counts = {}
    for event in event_store:
        event_type = event.type.value
        event_counts[event_type] = event_counts.get(event_type, 0) + 1

    return {
        "total_events": len(event_store),
        "event_counts_by_type": event_counts,
        "total_subscriptions": len(subscriptions),
        "rabbitmq_connected": event_bus.rabbitmq_connection is not None,
    }


# --- Specific Event Triggers (for testing) ---


@app.post("/trigger/journal-entry-created")
async def trigger_journal_entry_created(entry_id: str, amount: float, description: str = "Test journal entry"):
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
    return {"status": "triggered", "event_id": event.id}


@app.post("/trigger/budget-variance-alert")
async def trigger_budget_variance_alert(budget_id: str, variance_percent: float, category: str):
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
    return {"status": "triggered", "event_id": event.id}


@app.post("/trigger/transaction-flagged")
async def trigger_transaction_flagged(transaction_id: str, fraud_score: float, reason: str):
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
    return {"status": "triggered", "event_id": event.id}


@app.post("/trigger/approval-requested")
async def trigger_approval_requested(approval_id: str, requester: str, approvers: List[str], amount: float):
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
    return {"status": "triggered", "event_id": event.id}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8097)
