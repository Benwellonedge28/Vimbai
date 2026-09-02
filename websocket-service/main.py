"""Vimbai WebSocket Service - Real-time WebSocket connections for live updates. Port: 8365"""

import asyncio
import json
import os
from collections import defaultdict
from datetime import datetime, timezone
from typing import Any, Dict, List, Set

import structlog
from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.middleware.cors import CORSMiddleware

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
_message_log: List[dict] = []


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "active_channels": manager.stats()}


@app.get("/stats")
async def stats():
    return {
        "connections": manager.stats(),
        "total_connections": sum(len(v) for v in manager.active.values()),
        "messages_sent": len(_message_log),
    }


@app.websocket("/ws/{channel}")
async def websocket_endpoint(ws: WebSocket, channel: str):
    await manager.connect(ws, channel)
    try:
        while True:
            data = await ws.receive_text()
            msg = {"channel": channel, "data": data, "timestamp": datetime.now(timezone.utc).isoformat()}
            _message_log.append(msg)
            await manager.broadcast(channel, msg)
    except WebSocketDisconnect:
        manager.disconnect(ws, channel)
        await manager.broadcast(
            channel, {"type": "disconnect", "channel": channel, "timestamp": datetime.now(timezone.utc).isoformat()}
        )


@app.post("/broadcast/{channel}")
async def broadcast_to_channel(channel: str, message: dict):
    msg = {
        "channel": channel,
        "data": message,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "type": "broadcast",
    }
    _message_log.append(msg)
    await manager.broadcast(channel, msg)
    return {"channel": channel, "delivered_to": len(manager.active.get(channel, set()))}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
