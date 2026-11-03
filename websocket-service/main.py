"""Vimbai WebSocket Service - Real-time WebSocket connections for live updates. Port: 8365"""

import asyncio
import json
import os
import sys
from collections import defaultdict
from datetime import datetime, timezone
from typing import Any, Dict, List, Set

import structlog
from fastapi import FastAPI, Request, WebSocket, WebSocketDisconnect
from fastapi.middleware.cors import CORSMiddleware

# Loaded bare (uvicorn main:app) and by path (root integration tests), so this
# directory has to be importable before the relay module is pulled in.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import realtime  # noqa: E402

SERVICE_NAME = "websocket-service"
PORT = int(os.getenv("PORT", "8365"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai WebSocket Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="websocket-service", instrument_app=app)
except ImportError:
    TRACER = None


class ConnectionManager:
    def __init__(self):
        self.active: Dict[str, Set[WebSocket]] = defaultdict(set)

    async def connect(self, ws: WebSocket, channel: str):
        await ws.accept()
        self.active[channel].add(ws)
        await ws.send_json(
            {"type": "connected", "channel": channel, "timestamp": datetime.now(timezone.utc).isoformat()}
        )

    def disconnect(self, ws: WebSocket, channel: str):
        self.active[channel].discard(ws)

    async def broadcast(self, channel: str, message: dict):
        dead = []
        for ws in self.active.get(channel, []):
            try:
                await ws.send_json(message)
            except Exception:
                dead.append(ws)
        for ws in dead:
            self.active[channel].discard(ws)

    def stats(self) -> dict:
        return {ch: len(conns) for ch, conns in self.active.items()}


manager = ConnectionManager()
dedupe = realtime.EventDeduplicator()


async def publish_event(event: Dict[str, Any]) -> int:
    """Broadcast one bus event to this pod's connections for its Book.

    Returns the number of sockets written to. Duplicate deliveries (the same
    event arriving over both RabbitMQ and HTTP ingest) are dropped by id.
    """
    global _messages_sent
    channel = realtime.book_channel(event)
    if not channel:
        return 0
    if dedupe.is_duplicate(event.get("id") or event.get("event_id")):
        return 0

    message = realtime.normalise(event)
    message["channel"] = channel
    targets = list(manager.active.get(channel, set()))
    _messages_sent += len(targets)
    await manager.broadcast(channel, message)
    return len(targets)


relay = realtime.RealtimeRelay(publish_event)


@app.on_event("startup")
async def start_realtime_relay():
    """Subscribe this pod to the Book event stream."""
    relay.start()


@app.on_event("shutdown")
async def stop_realtime_relay():
    await relay.stop()


# Lifetime delivery counter (the message payloads themselves are never
# persisted or re-read - bound this to an int, not an unbounded list).
_messages_sent = 0


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "active_channels": manager.stats()}


@app.get("/stats")
async def stats():
    return {
        "connections": manager.stats(),
        "total_connections": sum(len(v) for v in manager.active.values()),
        "messages_sent": _messages_sent,
        "relay": {"status": relay.status, "received": relay.received, "last_error": relay.last_error},
    }


@app.post("/events/ingest")
async def ingest_event(event: Dict[str, Any], request: Request):
    """Cluster-internal: deliver one bus event to this pod's WebSocket clients.

    The message bus calls this for every published event when the RabbitMQ
    fan-out is unavailable, and the integration tests use it directly. The
    event's own Book decides the channel; anything without a Book is dropped
    rather than broadcast to a guessed channel.
    """
    delivered = await publish_event(event)
    return {"delivered_to": delivered, "channel": realtime.book_channel(event)}


@app.websocket("/ws/{channel}")
async def websocket_endpoint(ws: WebSocket, channel: str):
    await manager.connect(ws, channel)
    global _messages_sent
    try:
        while True:
            data = await ws.receive_text()
            msg = {"channel": channel, "data": data, "timestamp": datetime.now(timezone.utc).isoformat()}
            _messages_sent += 1
            await manager.broadcast(channel, msg)
    except WebSocketDisconnect:
        manager.disconnect(ws, channel)
        await manager.broadcast(
            channel, {"type": "disconnect", "channel": channel, "timestamp": datetime.now(timezone.utc).isoformat()}
        )


@app.post("/broadcast/{channel}")
async def broadcast_to_channel(channel: str, message: dict):
    global _messages_sent
    msg = {
        "channel": channel,
        "data": message,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "type": "broadcast",
    }
    _messages_sent += 1
    await manager.broadcast(channel, msg)
    return {"channel": channel, "delivered_to": len(manager.active.get(channel, set()))}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
