"""
FinAcc Real-Time Alerts Service
Provides real-time monitoring and alerting for financial events and metrics
"""

from fastapi import FastAPI, WebSocket, WebSocketDisconnect, HTTPException, Depends, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any, Literal
from datetime import datetime, timedelta
from enum import Enum
import asyncio
import json
import uuid
import os
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Alerts Service",
    description="Real-time alerts and notifications for financial events",
    version="0.1.0",
)

# ============================================================================
# Models
# ============================================================================

class AlertSeverity(str, Enum):
    CRITICAL = "critical"
    HIGH = "high"
    MEDIUM = "medium"
    LOW = "low"
    INFO = "info"

class AlertCategory(str, Enum):
    FRAUD = "fraud"
    COMPLIANCE = "compliance"
    FINANCIAL = "financial"
    SECURITY = "security"
    SYSTEM = "system"
    WORKFLOW = "workflow"

class AlertStatus(str, Enum):
    ACTIVE = "active"
    ACKNOWLEDGED = "acknowledged"
    RESOLVED = "resolved"
    DISMISSED = "dismissed"

class AlertRuleCreate(BaseModel):
    name: str = Field(..., min_length=3, max_length=100)
    description: Optional[str] = None
    category: AlertCategory
    severity: AlertSeverity
    condition: Dict[str, Any] = Field(..., description="Alert condition definition")
    action: Literal["notify", "email", "webhook", "auto_resolve"] = "notify"
    action_config: Optional[Dict[str, Any]] = None
    enabled: bool = True
    cooldown_seconds: int = Field(default=300, ge=0)

class AlertRuleInDB(AlertRuleCreate):
    id: str
    created_by: str
    created_at: datetime
    updated_at: datetime
    trigger_count: int = 0

class AlertCreate(BaseModel):
    rule_id: str
    title: str
    message: str
    severity: AlertSeverity
    category: AlertCategory
    source: str
    metadata: Optional[Dict[str, Any]] = None

class AlertInDB(AlertCreate):
    id: str
    status: AlertStatus
    created_at: datetime
    acknowledged_at: Optional[datetime] = None
    resolved_at: Optional[datetime] = None

class AlertSubscription(BaseModel):
    user_id: str
    categories: List[AlertCategory] = []
    severities: List[AlertSeverity] = []
    webhook_url: Optional[str] = None

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
# Alert Rules Store (In-memory for demo, use database in production)
# ============================================================================

alert_rules: Dict[str, AlertRuleInDB] = {}
alerts: Dict[str, AlertInDB] = {}

# Pre-defined alert rules
default_rules = [
    AlertRuleInDB(
        id="fraud-high-amount",
        name="High Value Transaction Alert",
        description="Alert when transaction exceeds threshold",
        category=AlertCategory.FRAUD,
        severity=AlertSeverity.HIGH,
        condition={"type": "threshold", "field": "amount", "operator": "gt", "value": 10000},
        action="notify",
        created_by="system",
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow()
    ),
    AlertRuleInDB(
        id="fraud-unusual-pattern",
        name="Unusual Transaction Pattern",
        description="Alert for unusual transaction patterns",
        category=AlertCategory.FRAUD,
        severity=AlertSeverity.CRITICAL,
        condition={"type": "pattern", "pattern_type": "velocity", "threshold": 10},
        action="notify",
        created_by="system",
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow()
    ),
    AlertRuleInDB(
        id="compliance-deadline",
        name="Compliance Deadline Reminder",
        description="Alert for upcoming compliance deadlines",
        category=AlertCategory.COMPLIANCE,
        severity=AlertSeverity.MEDIUM,
        condition={"type": "deadline", "days_before": 7},
        action="email",
        created_by="system",
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow()
    ),
]

for rule in default_rules:
    alert_rules[rule.id] = rule

# ============================================================================
# Alert Evaluation Engine
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

# ============================================================================
# API Endpoints
# ============================================================================

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
                        webhook_url=message.get("webhook_url")
                    )
                    manager.subscribe(subscription)
                    await websocket.send_json({
                        "type": "subscription_confirmed",
                        "categories": [c.value for c in subscription.categories],
                        "severities": [s.value for s in subscription.severities]
                    })
                elif message.get("type") == "unsubscribe":
                    manager.unsubscribe(user_id)
                    await websocket.send_json({"type": "unsubscription_confirmed"})
            except (json.JSONDecodeError, ValueError):
                await websocket.send_json({
                    "type": "error",
                    "message": "Invalid message format"
                })
    except WebSocketDisconnect:
        await manager.disconnect(websocket, user_id)

# --- Alert Rules Endpoints ---
@app.post("/rules", response_model=AlertRuleInDB, status_code=status.HTTP_201_CREATED)
async def create_alert_rule(rule: AlertRuleCreate, user_id: str = "system"):
    """Create a new alert rule"""
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
        updated_at=now
    )

    alert_rules[rule_id] = db_rule
    return db_rule

@app.get("/rules", response_model=List[AlertRuleInDB])
async def list_alert_rules(enabled_only: bool = False):
    """List all alert rules"""
    rules = list(alert_rules.values())
    if enabled_only:
        rules = [r for r in rules if r.enabled]
    return rules

@app.get("/rules/{rule_id}", response_model=AlertRuleInDB)
async def get_alert_rule(rule_id: str):
    """Get a specific alert rule"""
    if rule_id not in alert_rules:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")
    return alert_rules[rule_id]

@app.put("/rules/{rule_id}", response_model=AlertRuleInDB)
async def update_alert_rule(rule_id: str, rule: AlertRuleCreate):
    """Update an alert rule"""
    if rule_id not in alert_rules:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")

    existing = alert_rules[rule_id]
    existing.name = rule.name
    existing.description = rule.description
    existing.category = rule.category
    existing.severity = rule.severity
    existing.condition = rule.condition
    existing.action = rule.action
    existing.action_config = rule.action_config
    existing.enabled = rule.enabled
    existing.cooldown_seconds = rule.cooldown_seconds
    existing.updated_at = datetime.utcnow()

    return existing

@app.delete("/rules/{rule_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_alert_rule(rule_id: str):
    """Delete an alert rule"""
    if rule_id in alert_rules:
        del alert_rules[rule_id]

# --- Alerts Endpoints ---
@app.post("/alerts", response_model=AlertInDB, status_code=status.HTTP_201_CREATED)
async def create_alert(alert: AlertCreate):
    """Create a new alert (internal use or webhook-triggered)"""
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
        created_at=now
    )

    alerts[alert_id] = db_alert

    # Broadcast via WebSocket
    alert_message = {
        "type": "alert",
        "alert": {
            "id": db_alert.id,
            "title": db_alert.title,
            "message": db_alert.message,
            "severity": db_alert.severity.value,
            "category": db_alert.category.value,
            "created_at": db_alert.created_at.isoformat()
        }
    }
    await manager.broadcast(
        alert_message,
        categories=[db_alert.category.value],
        severities=[db_alert.severity.value]
    )

    return db_alert

@app.get("/alerts", response_model=List[AlertInDB])
async def list_alerts(
    status: Optional[AlertStatus] = None,
    category: Optional[AlertCategory] = None,
    severity: Optional[AlertSeverity] = None,
    limit: int = Query(100, ge=1, le=1000)
):
    """List alerts with optional filters"""
    filtered = list(alerts.values())

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
async def get_alert(alert_id: str):
    """Get a specific alert"""
    if alert_id not in alerts:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")
    return alerts[alert_id]

@app.put("/alerts/{alert_id}/acknowledge", response_model=AlertInDB)
async def acknowledge_alert(alert_id: str):
    """Acknowledge an alert"""
    if alert_id not in alerts:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")

    alert = alerts[alert_id]
    alert.status = AlertStatus.ACKNOWLEDGED
    alert.acknowledged_at = datetime.utcnow()

    # Broadcast update
    await manager.broadcast({
        "type": "alert_update",
        "alert_id": alert_id,
        "status": AlertStatus.ACKNOWLEDGED.value,
        "acknowledged_at": alert.acknowledged_at.isoformat()
    })

    return alert

@app.put("/alerts/{alert_id}/resolve", response_model=AlertInDB)
async def resolve_alert(alert_id: str, resolution_note: Optional[str] = None):
    """Resolve an alert"""
    if alert_id not in alerts:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")

    alert = alerts[alert_id]
    alert.status = AlertStatus.RESOLVED
    alert.resolved_at = datetime.utcnow()

    if resolution_note and alert.metadata:
        alert.metadata["resolution_note"] = resolution_note

    # Broadcast update
    await manager.broadcast({
        "type": "alert_update",
        "alert_id": alert_id,
        "status": AlertStatus.RESOLVED.value,
        "resolved_at": alert.resolved_at.isoformat()
    })

    return alert

@app.put("/alerts/{alert_id}/dismiss", response_model=AlertInDB)
async def dismiss_alert(alert_id: str):
    """Dismiss an alert (mark as false positive)"""
    if alert_id not in alerts:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Alert not found")

    alert = alerts[alert_id]
    alert.status = AlertStatus.DISMISSED

    # Broadcast update
    await manager.broadcast({
        "type": "alert_update",
        "alert_id": alert_id,
        "status": AlertStatus.DISMISSED.value
    })

    return alert

# --- Alert Evaluation Endpoint ---
@app.post("/evaluate")
async def evaluate_data(data: Dict[str, Any]):
    """Evaluate data against all enabled alert rules and trigger matching alerts"""
    triggered = []

    for rule_id, rule in alert_rules.items():
        if not rule.enabled:
            continue

        # Check cooldown
        if hasattr(rule, '_last_triggered') and rule._last_triggered:
            elapsed = (datetime.utcnow() - rule._last_triggered).total_seconds()
            if elapsed < rule.cooldown_seconds:
                continue

        # Evaluate condition
        if alert_engine.evaluate(rule.condition, data):
            # Create alert
            alert_id = str(uuid.uuid4())
            now = datetime.utcnow()

            alert = AlertInDB(
                id=alert_id,
                rule_id=rule_id,
                title=f"{rule.name}: Threshold exceeded",
                message=f"Alert triggered for data: {json.dumps(data)}",
                severity=rule.severity,
                category=rule.category,
                source=data.get("source", "system"),
                metadata={"data": data, "rule_name": rule.name},
                status=AlertStatus.ACTIVE,
                created_at=now
            )

            alerts[alert_id] = alert
            rule.trigger_count += 1
            rule._last_triggered = now

            # Broadcast
            await manager.broadcast({
                "type": "alert",
                "alert": {
                    "id": alert.id,
                    "title": alert.title,
                    "message": alert.message,
                    "severity": alert.severity.value,
                    "category": alert.category.value,
                    "created_at": alert.created_at.isoformat()
                }
            }, categories=[alert.category.value], severities=[alert.severity.value])

            triggered.append(alert)

    return {"triggered_count": len(triggered), "alerts": triggered}

# --- Statistics Endpoint ---
@app.get("/stats")
async def get_alert_stats():
    """Get alert statistics"""
    total = len(alerts)
    by_status = {}
    by_severity = {}
    by_category = {}

    for alert in alerts.values():
        by_status[alert.status.value] = by_status.get(alert.status.value, 0) + 1
        by_severity[alert.severity.value] = by_severity.get(alert.severity.value, 0) + 1
        by_category[alert.category.value] = by_category.get(alert.category.value, 0) + 1

    return {
        "total_alerts": total,
        "by_status": by_status,
        "by_severity": by_severity,
        "by_category": by_category,
        "active_connections": len(manager.active_connections),
        "total_rules": len(alert_rules),
        "enabled_rules": sum(1 for r in alert_rules.values() if r.enabled)
    }

# --- Integration Endpoint for Internal Services ---
@app.post("/trigger/{rule_id}")
async def trigger_alert_by_rule(rule_id: str, data: Dict[str, Any]):
    """Trigger an alert based on a specific rule"""
    if rule_id not in alert_rules:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Rule not found")

    rule = alert_rules[rule_id]

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
        created_at=now
    )

    alerts[alert_id] = alert

    # Broadcast
    await manager.broadcast({
        "type": "alert",
        "alert": {
            "id": alert.id,
            "title": alert.title,
            "message": alert.message,
            "severity": alert.severity.value,
            "category": alert.category.value,
            "created_at": alert.created_at.isoformat()
        }
    })

    return alert


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8090)