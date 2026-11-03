"""Real-time fan-out for Book events.

The message bus already emits a `book.resource.*` event for every successful
write inside a Book. This module carries those events the last mile: to the
WebSocket connections held by *this* pod, and therefore to every signed-in
client of that Book.

Two transports, in order of preference:

1. RabbitMQ (aio_pika) - every pod declares its own anonymous, exclusive queue
   bound to the `vimbai_events` topic exchange, so a single publish reaches all
   replicas rather than whichever pod a k8s Service happened to pick.

2. HTTP ingest (`POST /events/ingest`) - used when aio_pika or the broker is
   unavailable, when the bus is configured with REALTIME_RELAY_MODE=http, and
   by the integration tests. Both transports may deliver the same event, so
   `EventDeduplicator` drops the second copy by event id.

Nothing in here can break a write: a dead broker or a slow client is logged and
skipped, never raised into the publishing path.
"""

from __future__ import annotations

import asyncio
import json
import os
from collections import OrderedDict
from datetime import datetime, timezone
from typing import Any, Callable, Dict, Optional

import structlog

logger = structlog.get_logger("websocket-service.realtime")

EXCHANGE_NAME = os.getenv("EVENT_EXCHANGE", "vimbai_events")


def _amqp_url() -> str:
    """Explicit RABBITMQ_URL wins; otherwise assemble it from the deployment parts."""
    explicit = os.getenv("RABBITMQ_URL")
    if explicit:
        return explicit
    host = os.getenv("RABBITMQ_HOST", "localhost")
    port = os.getenv("RABBITMQ_PORT", "5672")
    user = os.getenv("RABBITMQ_USER", "guest")
    password = os.getenv("RABBITMQ_PASS", "guest")
    return f"amqp://{user}:{password}@{host}:{port}/"


RABBITMQ_URL = _amqp_url()
ROUTING_KEY = os.getenv("REALTIME_ROUTING_KEY", "#")
DEDUPE_WINDOW = int(os.getenv("REALTIME_DEDUPE_WINDOW", "1000"))


def book_channel(event: Dict[str, Any]) -> Optional[str]:
    """Channel name a Book event belongs to, or None to drop the event.

    The Book is read from the payload first (the shape the bus and the gateway
    both send) and from the envelope as a fallback. Events without a Book are
    not broadcast: there is no safe channel to put them on, and a personal
    event must never leak into another tenant's stream.
    """
    payload = event.get("payload") or {}
    book_id = payload.get("book_id") or event.get("book_id") or (event.get("metadata") or {}).get("book_id")
    if not book_id:
        return None
    return f"book:{book_id}"


def normalise(event: Dict[str, Any]) -> Dict[str, Any]:
    """Shape the wire message a client sees: id, type, Book, resource, ts."""
    payload = event.get("payload") or {}
    return {
        "type": "event",
        "event_id": event.get("id") or event.get("event_id"),
        "event_type": event.get("event_type") or event.get("type"),
        "book_id": payload.get("book_id") or event.get("book_id"),
        "resource_type": payload.get("resource_type") or payload.get("resource"),
        "resource_id": payload.get("resource_id") or payload.get("id"),
        "action": payload.get("action"),
        "source_service": event.get("source_service"),
        "timestamp": event.get("timestamp") or datetime.now(timezone.utc).isoformat(),
        "payload": payload,
    }


class EventDeduplicator:
    """Bounded set of recently seen event ids (both transports can deliver one event)."""

    def __init__(self, window: int = DEDUPE_WINDOW):
        self.window = window
        self._seen: "OrderedDict[str, None]" = OrderedDict()

    def is_duplicate(self, event_id: Optional[str]) -> bool:
        if not event_id:
            return False
        if event_id in self._seen:
            self._seen.move_to_end(event_id)
            return True
        self._seen[event_id] = None
        while len(self._seen) > self.window:
            self._seen.popitem(last=False)
        return False


class RealtimeRelay:
    """Runs a RabbitMQ consumer for this pod and hands events to a broadcast fn."""

    def __init__(self, on_event: Callable[[Dict[str, Any]], Any]):
        self.on_event = on_event
        self._task: Optional[asyncio.Task] = None
        self._stopping = False
        self.status = "idle"  # idle | connecting | consuming | disabled | error
        self.last_error: Optional[str] = None
        self.received = 0

    def start(self) -> None:
        if self._task and not self._task.done():
            return
        self._stopping = False
        self._task = asyncio.create_task(self._run(), name="vimbai-realtime-relay")

    async def stop(self) -> None:
        self._stopping = True
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except (asyncio.CancelledError, Exception):  # noqa: BLE001 - shutdown is best effort
                pass
            self._task = None
        self.status = "idle"

    async def _run(self) -> None:
        try:
            import aio_pika  # noqa: F401 - optional dependency
        except ImportError:
            # HTTP ingest stays available, so real time still works single-pod.
            self.status = "disabled"
            self.last_error = "aio_pika not installed"
            logger.warning("realtime relay disabled: aio_pika not installed (HTTP ingest only)")
            return

        backoff = 1.0
        while not self._stopping:
            try:
                self.status = "connecting"
                await self._consume()
                backoff = 1.0
            except asyncio.CancelledError:
                raise
            except Exception as exc:  # noqa: BLE001 - broker outages must not kill the pod
                self.status = "error"
                self.last_error = str(exc)[:300]
                logger.warning("realtime relay consumer dropped, retrying", error=self.last_error, backoff=backoff)
                await asyncio.sleep(backoff)
                backoff = min(backoff * 2, 30.0)

    async def _consume(self) -> None:
        import aio_pika

        connection = await aio_pika.connect_robust(RABBITMQ_URL)
        async with connection:
            channel = await connection.channel()
            await channel.set_qos(prefetch_count=200)
            exchange = await channel.declare_exchange(EXCHANGE_NAME, aio_pika.ExchangeType.TOPIC, durable=True)

            # Anonymous queue: every pod gets its own copy of every event, so a
            # publish reaches clients on all replicas, not one lucky pod.
            queue = await channel.declare_queue(exclusive=True, auto_delete=True)
            await queue.bind(exchange, routing_key=ROUTING_KEY)

            self.status = "consuming"
            logger.info("realtime relay consuming", exchange=EXCHANGE_NAME, routing_key=ROUTING_KEY)

            async with queue.iterator() as events:
                async for message in events:
                    if self._stopping:
                        break
                    async with message.process(requeue=False):
                        try:
                            event = json.loads(message.body.decode() or "{}")
                        except (ValueError, UnicodeDecodeError):
                            logger.warning("realtime relay dropped an undecodable event")
                            continue
                        self.received += 1
                        try:
                            await self.on_event(event)
                        except Exception:  # noqa: BLE001 - one bad broadcast never stops the stream
                            logger.warning("realtime broadcast failed", exc_info=True)
