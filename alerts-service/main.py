"""Vimbai Real-Time Alerts Service. Port: 8090.

Real-time monitoring and alerting for financial events and metrics.
The two durable stores (alert rules, alerts) were process-global dicts
shared across ALL callers; they now persist to Neo4j as caller-owned,
Book-scoped records (X-User-Id / X-Book-ID). WebSocket connection and
subscription state is ephemeral session state and stays in memory.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "alerts_service" not in _sys.modules or not hasattr(_sys.modules.get("alerts_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("alerts_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["alerts_service"] = _pkg
    _sys.modules["alerts_service"].__path__ = [_HERE]

import asyncio
import json
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

from dotenv import load_dotenv
from fastapi import Depends, FastAPI, HTTPException, Query, Request, WebSocket, WebSocketDisconnect, status
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from pydantic import BaseModel

from alerts_service import crud
from alerts_service.dependencies import book_id_var, get_db_session, get_user_id
from alerts_service.exceptions import AlertsError
from alerts_service.models import (
    AlertCategory,
    AlertCreate,
    AlertInDB,
    AlertRuleCreate,
    AlertRuleInDB,
    AlertSeverity,
    AlertStatus,
    AlertSubscription,
)

load_dotenv()

app = FastAPI(
    title="Vimbai Alerts Service",
    description="Real-time alerts and notifications for financial events",
    version="0.1.0",
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(AlertsError)
async def _alerts_error(request: Request, exc: AlertsError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400), content={"detail": str(exc), "error": exc.__class__.__name__}
    )


# ============================================================================
# Connection Manager for WebSocket
# ============================================================================


class ConnectionManager:
    """Manages WebSocket connections for real-time alerts"""

    def __init__(self):
        # Active connections by user_id
        self.active_connections: Dict[str, List[WebSocket]] = {}
        # Alert subscriptions by user_id
        self.subscriptions: Dict[str, AlertSubscription] = {}
        # Lock for thread-safe operations
        self.lock = asyncio.Lock()

    async def connect(self, websocket: WebSocket, user_id: str):
        await websocket.accept()
        async with self.lock:
            if user_id not in self.active_connections:
                self.active_connections[user_id] = []
            self.active_connections[user_id].append(websocket)

    async def disconnect(self, websocket: WebSocket, user_id: str):
        async with self.lock:
            if user_id in self.active_connections:
                if websocket in self.active_connections[user_id]:
                    self.active_connections[user_id].remove(websocket)
                if not self.active_connections[user_id]:
                    del self.active_connections[user_id]

    async def send_personal_message(self, message: dict, user_id: str):
        async with self.lock:
            if user_id in self.active_connections:
                disconnected = []
                for connection in self.active_connections[user_id]:
                    try:
                        await connection.send_json(message)
                    except Exception:
                        disconnected.append(connection)
                # Clean up disconnected
                for conn in disconnected:
                    await self.disconnect(conn, user_id)

    async def broadcast(self, message: dict, categories: List[str] = None, severities: List[str] = None):
        """Broadcast alert to all connected users based on their subscriptions"""
        async with self.lock:
            for user_id, connections in self.active_connections.items():
                subscription = self.subscriptions.get(user_id)
                # Check if user should receive this alert
                should_send = True
                if subscription:
                    if categories and subscription.categories:
                        if not any(cat.value in categories for cat in subscription.categories):
                            should_send = False
                    if severities and subscription.severities:
                        if not any(sev.value in severities for sev in subscription.severities):
                            should_send = False

                if should_send:
                    for connection in connections:
                        try:
                            await connection.send_json(message)
                        except Exception:
                            pass

    def subscribe(self, subscription: AlertSubscription):
        self.subscriptions[subscription.user_id] = subscription

    def unsubscribe(self, user_id: str):
        if user_id in self.subscriptions:
            del self.subscriptions[user_id]


manager = ConnectionManager()


# ============================================================================
# Alert Engine
# ============================================================================


class AlertEngine:
    """Evaluates conditions and triggers alerts"""

    @staticmethod
    def evaluate_threshold(condition: dict, data: dict) -> bool:
        """Evaluate threshold conditions"""
        field = condition.get("field")
        operator = condition.get("operator")
        threshold = condition.get("value")

        if not all([field, operator, threshold]):
            return False

        value = data.get(field)
        if value is None:
            return False

        try:
            value = float(value)
            threshold = float(threshold)

            operators = {
                "gt": lambda v, t: v > t,
                "gte": lambda v, t: v >= t,
                "lt": lambda v, t: v < t,
                "lte": lambda v, t: v <= t,
                "eq": lambda v, t: v == t,
                "neq": lambda v, t: v != t,
            }

            if operator in operators:
                return operators[operator](value, threshold)
        except (ValueError, TypeError):
            pass

        return False

    @staticmethod
    def evaluate_pattern(condition: dict, data: dict) -> bool:
        """Evaluate pattern-based conditions"""
        pattern_type = condition.get("pattern_type")

        if pattern_type == "velocity":
            # Check transaction velocity
            threshold = condition.get("threshold", 10)
            count = data.get("transaction_count", 0)
            window = data.get("time_window_minutes", 60)
            if count > threshold and window <= 60:
                return True

        return False

    @staticmethod
    def evaluate(condition: dict, data: dict) -> bool:
        """Evaluate any condition type"""
        condition_type = condition.get("type")

        if condition_type == "threshold":
            return AlertEngine.evaluate_threshold(condition, data)
        elif condition_type == "pattern":
            return AlertEngine.evaluate_pattern(condition, data)
        elif condition_type == "deadline":
            # Check if deadline is approaching
            days_before = condition.get("days_before", 7)
            deadline = data.get("deadline")
            if deadline:
                try:
                    deadline_date = datetime.fromisoformat(deadline)
                    days_until = (deadline_date - datetime.utcnow()).days
                    return 0 <= days_until <= days_before
                except (ValueError, TypeError):
                    pass

        return False


alert_engine = AlertEngine()


@app.on_event("startup")
async def startup():
    """Initialize service"""
    print("Alerts service started")


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "alerts"}


# --- WebSocket Endpoint ---

@app.websocket("/ws/alerts/{user_id}")
async def websocket_alerts(websocket: WebSocket, user_id: str):
    """WebSocket endpoint for real-time alerts"""
    await manager.connect(websocket, user_id)
    try:
        while True:
            # Receive messages from client (e.g., subscription updates)
            data = await websocket.receive_text()
            try:
                message = json.loads(data)
                if message.get("type") == "subscribe":
                    # Handle subscription
                    subscription = AlertSubscription(
                        user_id=user_id,
                        categories=[AlertCategory(c) for c in message.get("categories", [])],
                        severities=[AlertSeverity(s) for s in message.get("severities", [])],
                        webhook_url=message.get("webhook_url"),
                    )
                    manager.subscribe(subscription)
                    await websocket.send_json(
                        {
                            "type": "subscription_confirmed",
                            "categories": [c.value for c in subscription.categories],
                            "severities": [s.value for s in subscription.severities],
                        }
                    )
                elif message.get("type") == "unsubscribe":
                    manager.unsubscribe(user_id)
                    await websocket.send_json({"type": "unsubscription_confirmed"})
            except (json.JSONDecodeError, ValueError):
                await websocket.send_json({"type": "error", "message": "Invalid message format"})
    except WebSocketDisconnect:
        await manager.disconnect(websocket, user_id)


# --- Alert Rules Endpoints ---


@app.post("/rules", response_model=AlertRuleInDB, status_code=status.HTTP_201_CREATED)
async def create_alert_rule(
    rule: AlertRuleCreate,
    user_id: str = "system",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new alert rule (owned by the caller)"""
    rule_id = str(uuid.uuid4())
    now = datetime.utcnow()

    db_rule = AlertRuleInDB(
        id=rule_id,
        name=rule.name,
        description=rule.description,
        category=rule.category,
        severity=rule.severity,
        condition=rule.condition,
        action=rule.action,
        action_config=rule.action_config,
        enabled=rule.enabled,
        cooldown_seconds=rule.cooldown_seconds,
        created_by=user_id,
        created_at=now,
        updated_at=now,
    )

    await crud.create(db_session, caller_id, db_rule, extra={"last_triggered": None})
    return db_rule


@app.get("/rules", response_model=List[AlertRuleInDB])
async def list_alert_rules(
    enabled_only: bool = False,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's alert rules"""
    rules = await crud.list_all(db_session, caller_id, AlertRuleInDB)
    if enabled_only:
        rules = [r for r in rules if r.enabled]
    return rules


@app.get("/rules/{rule_id}", response_model=AlertRuleInDB)
async def get_alert_rule(
    rule_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific alert rule; cross-scope 404"""
    rule, _ = await crud.find(db_session, caller_id, AlertRuleInDB, rule_id)
    if not rule:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")
    return rule


@app.put("/rules/{rule_id}", response_model=AlertRuleInDB)
async def update_alert_rule(
    rule_id: str,
    rule: AlertRuleCreate,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update an alert rule (caller-owned)"""
    existing, extras = await crud.find(db_session, caller_id, AlertRuleInDB, rule_id)
    if not existing:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")

    updated = AlertRuleInDB(
        id=existing.id,
        name=rule.name,
        description=rule.description,
        category=rule.category,
        severity=rule.severity,
        condition=rule.condition,
        action=rule.action,
        action_config=rule.action_config,
        enabled=rule.enabled,
        cooldown_seconds=rule.cooldown_seconds,
        created_by=existing.created_by,
        created_at=existing.created_at,
        updated_at=datetime.utcnow(),
        trigger_count=existing.trigger_count,
    )

    # latest-wins upsert, preserving cooldown bookkeeping
    await crud.delete_where(db_session, caller_id, AlertRuleInDB, {"id": rule_id})
    await crud.create(db_session, caller_id, updated, extra={"last_triggered": extras.get("last_triggered")})
    return updated


@app.delete("/rules/{rule_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_alert_rule(
    rule_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete an alert rule (caller-scoped; 204 like the original for unknown ids)"""
    await crud.delete_where(db_session, caller_id, AlertRuleInDB, {"id": rule_id})


# --- Alerts Endpoints ---


@app.post("/alerts", response_model=AlertInDB, status_code=status.HTTP_201_CREATED)
async def create_alert(
    alert: AlertCreate,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new alert (internal use or webhook-triggered; caller-owned)"""
    alert_id = str(uuid.uuid4())
    now = datetime.utcnow()

    db_alert = AlertInDB(
        id=alert_id,
        rule_id=alert.rule_id,
        title=alert.title,
        message=alert.message,
        severity=alert.severity,
        category=alert.category,
        source=alert.source,
        metadata=alert.metadata,
        status=AlertStatus.ACTIVE,
        created_at=now,
    )

    await crud.create(db_session, caller_id, db_alert)

    # Broadcast via WebSocket
    alert_message = {
        "type": "alert",
        "alert": {
            "id": db_alert.id,
            "title": db_alert.title,
            "message": db_alert.message,
            "severity": db_alert.severity.value,
            "category": db_alert.category.value,
            "created_at": db_alert.created_at.isoformat(),
        },
    }
    await manager.broadcast(alert_message, categories=[db_alert.category.value], severities=[db_alert.severity.value])

    return db_alert


@app.get("/alerts", response_model=List[AlertInDB])
async def list_alerts(
    status: Optional[AlertStatus] = None,
    category: Optional[AlertCategory] = None,
    severity: Optional[AlertSeverity] = None,
    limit: int = Query(100, ge=1, le=1000),
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's alerts with optional filters"""
    filtered = await crud.list_all(db_session, caller_id, AlertInDB)

    if status:
        filtered = [a for a in filtered if a.status == status]
    if category:
        filtered = [a for a in filtered if a.category == category]
    if severity:
        filtered = [a for a in filtered if a.severity == severity]

    # Sort by created_at descending
    filtered.sort(key=lambda x: x.created_at, reverse=True)

    return filtered[:limit]


@app.get("/alerts/{alert_id}", response_model=AlertInDB)
async def get_alert(
    alert_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific alert; cross-scope 404"""
    alert, _ = await crud.find(db_session, caller_id, AlertInDB, alert_id)
    if not alert:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")
    return alert


@app.put("/alerts/{alert_id}/acknowledge", response_model=AlertInDB)
async def acknowledge_alert(
    alert_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Acknowledge an alert"""
    alert, _ = await crud.find(db_session, caller_id, AlertInDB, alert_id)
    if not alert:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")

    alert.status = AlertStatus.ACKNOWLEDGED
    alert.acknowledged_at = datetime.utcnow()
    await crud.update_props(
        db_session, caller_id, AlertInDB, alert_id,
        {"status": alert.status, "acknowledged_at": alert.acknowledged_at},
    )

    # Broadcast update
    await manager.broadcast(
        {
            "type": "alert_update",
            "alert_id": alert_id,
            "status": AlertStatus.ACKNOWLEDGED.value,
            "acknowledged_at": alert.acknowledged_at.isoformat(),
        }
    )

    return alert


@app.put("/alerts/{alert_id}/resolve", response_model=AlertInDB)
async def resolve_alert(
    alert_id: str,
    resolution_note: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Resolve an alert"""
    alert, _ = await crud.find(db_session, caller_id, AlertInDB, alert_id)
    if not alert:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")

    alert.status = AlertStatus.RESOLVED
    alert.resolved_at = datetime.utcnow()

    if resolution_note and alert.metadata:
        alert.metadata["resolution_note"] = resolution_note

    updates: Dict[str, Any] = {"status": alert.status, "resolved_at": alert.resolved_at}
    if resolution_note and alert.metadata:
        updates["metadata"] = alert.metadata
    await crud.update_props(db_session, caller_id, AlertInDB, alert_id, updates)

    # Broadcast update
    await manager.broadcast(
        {
            "type": "alert_update",
            "alert_id": alert_id,
            "status": AlertStatus.RESOLVED.value,
            "resolved_at": alert.resolved_at.isoformat(),
        }
    )

    return alert


@app.put("/alerts/{alert_id}/dismiss", response_model=AlertInDB)
async def dismiss_alert(
    alert_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Dismiss an alert (mark as false positive)"""
    alert, _ = await crud.find(db_session, caller_id, AlertInDB, alert_id)
    if not alert:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")

    alert.status = AlertStatus.DISMISSED
    await crud.update_props(db_session, caller_id, AlertInDB, alert_id, {"status": alert.status})

    # Broadcast update
    await manager.broadcast({"type": "alert_update", "alert_id": alert_id, "status": AlertStatus.DISMISSED.value})

    return alert


# --- Alert Evaluation Endpoint ---


@app.post("/evaluate")
async def evaluate_data(
    data: Dict[str, Any],
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Evaluate data against the caller's enabled alert rules and trigger matching alerts"""
    triggered = []

    for rule, extras in [
        (r, (await crud.find(db_session, caller_id, AlertRuleInDB, r.id))[1])
        for r in await crud.list_all(db_session, caller_id, AlertRuleInDB)
    ]:
        if not rule.enabled:
            continue

        # Check cooldown (persisted last_triggered bookkeeping)
        last_triggered = crud._coerce_dt(extras.get("last_triggered"))
        if last_triggered:
            elapsed = (datetime.utcnow() - last_triggered).total_seconds()
            if elapsed < rule.cooldown_seconds:
                continue

        # Evaluate condition
        if alert_engine.evaluate(rule.condition, data):
            # Create alert
            alert_id = str(uuid.uuid4())
            now = datetime.utcnow()

            alert = AlertInDB(
                id=alert_id,
                rule_id=rule.id,
                title=f"{rule.name}: Threshold exceeded",
                message=f"Alert triggered for data: {json.dumps(data)}",
                severity=rule.severity,
                category=rule.category,
                source=data.get("source", "system"),
                metadata={"data": data, "rule_name": rule.name},
                status=AlertStatus.ACTIVE,
                created_at=now,
            )

            await crud.create(db_session, caller_id, alert)
            rule.trigger_count += 1
            await crud.update_props(
                db_session, caller_id, AlertRuleInDB, rule.id,
                {"trigger_count": rule.trigger_count, "last_triggered": now},
            )

            # Broadcast
            await manager.broadcast(
                {
                    "type": "alert",
                    "alert": {
                        "id": alert.id,
                        "title": alert.title,
                        "message": alert.message,
                        "severity": alert.severity.value,
                        "category": alert.category.value,
                        "created_at": alert.created_at.isoformat(),
                    },
                },
                categories=[alert.category.value],
                severities=[alert.severity.value],
            )

            triggered.append(alert)

    return {"triggered_count": len(triggered), "alerts": triggered}


# --- Statistics Endpoint ---


@app.get("/stats")
async def get_alert_stats(
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get alert statistics for the caller's alerts only"""
    all_alerts = await crud.list_all(db_session, caller_id, AlertInDB)
    all_rules = await crud.list_all(db_session, caller_id, AlertRuleInDB)

    by_status: Dict[str, int] = {}
    by_severity: Dict[str, int] = {}
    by_category: Dict[str, int] = {}

    for alert in all_alerts:
        by_status[alert.status.value] = by_status.get(alert.status.value, 0) + 1
        by_severity[alert.severity.value] = by_severity.get(alert.severity.value, 0) + 1
        by_category[alert.category.value] = by_category.get(alert.category.value, 0) + 1

    return {
        "total_alerts": len(all_alerts),
        "by_status": by_status,
        "by_severity": by_severity,
        "by_category": by_category,
        "active_connections": len(manager.active_connections),
        "total_rules": len(all_rules),
        "enabled_rules": sum(1 for r in all_rules if r.enabled),
    }


# --- Integration Endpoint for Internal Services ---


@app.post("/trigger/{rule_id}", response_model=AlertInDB)
async def trigger_alert_by_rule(
    rule_id: str,
    data: Dict[str, Any],
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Trigger an alert based on a caller-owned rule"""
    rule, _ = await crud.find(db_session, caller_id, AlertRuleInDB, rule_id)
    if not rule:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")

    alert_id = str(uuid.uuid4())
    now = datetime.utcnow()

    alert = AlertInDB(
        id=alert_id,
        rule_id=rule_id,
        title=rule.name,
        message=data.get("message", f"Alert triggered by rule: {rule.name}"),
        severity=rule.severity,
        category=rule.category,
        source=data.get("source", "internal"),
        metadata=data,
        status=AlertStatus.ACTIVE,
        created_at=now,
    )

    await crud.create(db_session, caller_id, alert)

    # Broadcast
    await manager.broadcast(
        {
            "type": "alert",
            "alert": {
                "id": alert.id,
                "title": alert.title,
                "message": alert.message,
                "severity": alert.severity.value,
                "category": alert.category.value,
                "created_at": alert.created_at.isoformat(),
            },
        }
    )

    return alert


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8090)
