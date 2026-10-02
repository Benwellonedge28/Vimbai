"""
Vimbai Notification Service
Handles notifications for workflows, approvals, and system events.

This file may be imported bare (uvicorn main:app), so it bootstraps
its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "notifications_service" not in _sys.modules or not hasattr(_sys.modules.get("notifications_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("notifications_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["notifications_service"] = _pkg
    _sys.modules["notifications_service"].__path__ = [_HERE]

import asyncio
import json
import os
import uuid
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

from dotenv import load_dotenv
from fastapi import Depends, FastAPI, HTTPException, Query, Request, WebSocket, WebSocketDisconnect, status
from neo4j import AsyncSession
from notifications_service import crud, models
from notifications_service.dependencies import book_id_var, get_db_session, get_user_id
from pydantic import BaseModel, Field

load_dotenv()

SERVICE_NAME = "notifications"

app = FastAPI(
    title="Vimbai Notifications Service",
    description="Notification and messaging system for Vimbai workflows",
    version="2.0.0",
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


# ============================================================================
# WebSocket Connection Manager (live sockets only - by design in-memory)
# ============================================================================


class NotificationManager:
    """Manages live notification connections and channel delivery."""

    def __init__(self):
        self.active_connections: Dict[str, List[WebSocket]] = defaultdict(list)
        self.lock = asyncio.Lock()

    async def connect(self, websocket: WebSocket, user_id: str, unread_count: int = 0):
        await websocket.accept()
        async with self.lock:
            self.active_connections[user_id].append(websocket)
        if unread_count:
            await websocket.send_json({"type": "unread_count", "count": unread_count})

    async def disconnect(self, websocket: WebSocket, user_id: str):
        async with self.lock:
            if user_id in self.active_connections:
                try:
                    self.active_connections[user_id].remove(websocket)
                except ValueError:
                    pass
                if not self.active_connections[user_id]:
                    del self.active_connections[user_id]

    async def send_websocket(self, user_id: str, message: dict):
        """Send message via WebSocket to a user's live sockets."""
        if user_id in self.active_connections:
            disconnected = []
            for ws in self.active_connections[user_id]:
                try:
                    await ws.send_json(message)
                except Exception:
                    disconnected.append(ws)
            async with self.lock:
                for ws in disconnected:
                    try:
                        self.active_connections[user_id].remove(ws)
                    except ValueError:
                        pass

    async def _send_via_channel(self, notification: models.NotificationInDB, user_id: str, channel):
        """Send notification via specific channel"""
        if channel == models.NotificationChannel.EMAIL:
            await self._send_email(notification, user_id)
        elif channel == models.NotificationChannel.WEBHOOK:
            await self._send_webhook(notification, user_id)
        elif channel == models.NotificationChannel.PUSH:
            await self._send_push(notification, user_id)
        elif channel == models.NotificationChannel.SMS:
            await self._send_sms(notification, user_id)

    async def _send_email(self, notification, user_id):
        """Send email notification (placeholder - integrate with email service)"""
        print(f"Email to {user_id}: {notification.title}")

    async def _send_webhook(self, notification, user_id):
        """Send webhook notification"""
        print(f"Webhook to {user_id}: {notification.title}")

    async def _send_push(self, notification, user_id):
        """Send push notification"""
        print(f"Push to {user_id}: {notification.title}")

    async def _send_sms(self, notification, user_id):
        """Send SMS notification"""
        print(f"SMS to {user_id}: {notification.message[:50]}")

    def _serialize_notification(self, notification) -> dict:
        return {
            "id": notification.id,
            "type": notification.type.value,
            "title": notification.title,
            "message": notification.message,
            "priority": notification.priority.value,
            "metadata": notification.metadata,
            "action_url": notification.action_url,
            "created_at": notification.created_at.isoformat(),
            "status": notification.status,
        }


notification_manager = NotificationManager()

# ============================================================================
# Notification Templates (static, shared definitions)
# ============================================================================

templates = {
    "approval_request": models.NotificationTemplate(
        name="Approval Request",
        type=models.NotificationType.APPROVAL_REQUIRED,
        subject_template="Approval Required: {{title}}",
        body_template="You have a new approval request for '{{title}}' from {{sender}}. Please review and take action.",
        variables=["title", "sender", "action_url"],
    ),
    "approval_completed": models.NotificationTemplate(
        name="Approval Completed",
        type=models.NotificationType.APPROVAL_COMPLETED,
        subject_template="Approved: {{title}}",
        body_template="Your request '{{title}}' has been approved by {{approver}}.",
        variables=["title", "approver"],
    ),
    "comment_added": models.NotificationTemplate(
        name="Comment Added",
        type=models.NotificationType.COMMENT_ADDED,
        subject_template="{{sender}} commented on {{title}}",
        body_template="{{sender}} added a comment: {{comment}}",
        variables=["sender", "title", "comment"],
    ),
    "mention": models.NotificationTemplate(
        name="Mention",
        type=models.NotificationType.MENTION,
        subject_template="{{sender}} mentioned you",
        body_template="{{sender}} mentioned you in '{{title}}': {{comment}}",
        variables=["sender", "title", "comment"],
    ),
    "deadline_reminder": models.NotificationTemplate(
        name="Deadline Reminder",
        type=models.NotificationType.DEADLINE_REMINDER,
        subject_template="Deadline Reminder: {{title}}",
        body_template="Reminder: '{{title}}' is due on {{deadline}}.",
        variables=["title", "deadline"],
    ),
}

# ============================================================================
# API Endpoints
# ============================================================================


@app.on_event("startup")
async def startup():
    print("Notifications service started")


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME}


# --- WebSocket Endpoint ---
@app.websocket("/ws/notifications/{user_id}")
async def websocket_notifications(websocket: WebSocket, user_id: str):
    """WebSocket endpoint for real-time notifications (per-user inbox socket)."""
    # Websocket connections bypass the HTTP middleware; bind the Book context here.
    book_id_var.set(websocket.headers.get("X-Book-ID"))
    from notifications_service.database import Neo4jConnector

    async with Neo4jConnector.get_driver().session() as session:
        try:
            unread_count = await crud.get_unread_count(session, user_id)
        except Exception:
            unread_count = 0
    await notification_manager.connect(websocket, user_id, unread_count=unread_count)
    try:
        while True:
            data = await websocket.receive_text()
            try:
                message = json.loads(data)
                async with Neo4jConnector.get_driver().session() as session:
                    if message.get("type") == "mark_read":
                        # Mark one of THIS socket user's notifications read
                        # (previously scanned every user's notifications).
                        notification_id = message.get("notification_id")
                        await crud.mark_notification_read(session, user_id, notification_id)
                        await notification_manager.send_websocket(
                            user_id, {"type": "notification_read", "notification_id": notification_id}
                        )
                    elif message.get("type") == "mark_all_read":
                        count = await crud.mark_all_read(session, user_id)
                        await notification_manager.send_websocket(user_id, {"type": "all_read", "marked_count": count})
            except json.JSONDecodeError:
                pass
    except WebSocketDisconnect:
        await notification_manager.disconnect(websocket, user_id)


# --- Send Notification ---
@app.post("/notifications", response_model=models.NotificationInDB, status_code=status.HTTP_201_CREATED)
async def create_notification(
    notification: models.NotificationCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create and send a notification (sender is the verified caller)."""
    now = datetime.now(timezone.utc)

    db_notification = models.NotificationInDB(
        id=str(uuid.uuid4()),
        type=notification.type,
        title=notification.title,
        message=notification.message,
        priority=notification.priority,
        recipients=notification.recipients,
        channels=notification.channels,
        metadata=notification.metadata,
        action_url=notification.action_url,
        scheduled_at=notification.scheduled_at,
        expires_at=notification.expires_at,
        sender=user_id,
        status="pending",
        created_at=now,
    )

    # Send to all recipients (delivery to arbitrary recipients is this
    # service's job; each recipient owns their own copy of the record).
    for recipient in notification.recipients:
        db_notification.status = "sent"
        db_notification.sent_at = now
        await crud.create_notification(db_session, recipient, db_notification, book_id=book_id_var.get())
        await notification_manager.send_websocket(
            recipient,
            {"type": "notification", "notification": notification_manager._serialize_notification(db_notification)},
        )
        for channel in notification.channels:
            await notification_manager._send_via_channel(db_notification, recipient, channel)

    db_notification.status = "sent"
    db_notification.sent_at = now

    return db_notification


# --- Send Batch Notifications ---
@app.post("/notifications/batch", status_code=status.HTTP_201_CREATED)
async def create_batch_notifications(
    notifications: List[models.NotificationCreate],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create and send multiple notifications"""
    results = []
    for notification in notifications:
        sent = await create_notification(notification, user_id, db_session)
        results.append(sent)

    return {"count": len(results), "notifications": results}


# --- Get User Notifications (own inbox only) ---
@app.get("/notifications/{user_id}")
async def get_user_notifications(
    user_id: str,
    unread_only: bool = Query(False),
    limit: int = Query(50, ge=1, le=200),
    caller: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get notifications for a user (the caller's own inbox only)."""
    if user_id != caller:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Access denied")

    notifications = await crud.list_notifications(db_session, user_id, unread_only, limit)
    unread_count = await crud.get_unread_count(db_session, user_id)

    return {
        "notifications": notifications,
        "unread_count": unread_count,
        "total_count": len(await crud.list_notifications(db_session, user_id, limit=200)),
    }


# --- Mark as Read ---
@app.put("/notifications/{notification_id}/read")
async def mark_notification_read(
    notification_id: str,
    caller: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Mark one of the caller's notifications as read"""
    notification = await crud.find_notification(db_session, caller, notification_id)
    if notification is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Notification not found")
    await crud.mark_notification_read(db_session, caller, notification_id)
    return {"status": "success", "notification_id": notification_id}


# --- Mark All as Read ---
@app.put("/notifications/{user_id}/read-all")
async def mark_all_notifications_read(
    user_id: str,
    caller: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Mark all the caller's notifications as read"""
    if user_id != caller:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Access denied")
    count = await crud.mark_all_read(db_session, user_id)
    return {"status": "success", "marked_count": count}


# --- Delete Notification ---
@app.delete("/notifications/{notification_id}")
async def delete_notification(
    notification_id: str,
    caller: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a notification from the caller's inbox"""
    notification = await crud.find_notification(db_session, caller, notification_id)
    if notification is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Notification not found")
    await crud.delete_notification(db_session, caller, notification_id)
    return {"status": "success"}


# --- Notification Preferences (personal; caller-gated) ---
@app.put("/preferences/{user_id}")
async def update_preferences(
    preferences: models.NotificationPreferences,
    user_id: str,
    caller: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update notification preferences for the caller"""
    if user_id != caller:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Access denied")
    preferences.user_id = user_id
    saved = await crud.save_preferences(db_session, user_id, preferences)
    return {"status": "success", "preferences": saved}


@app.get("/preferences/{user_id}")
async def get_preferences(
    user_id: str,
    caller: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get notification preferences for the caller (defaults when never set)"""
    if user_id != caller:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Access denied")
    stored = await crud.get_preferences(db_session, user_id)
    if stored is None:
        stored = models.NotificationPreferences(user_id=user_id, email_batch=True, email_batch_interval_minutes=60)
    return stored


# --- Template Endpoints ---
@app.get("/templates")
async def list_templates():
    """List all notification templates"""
    return [{"name": k, "template": v} for k, v in templates.items()]


@app.get("/templates/{template_name}")
async def get_template(template_name: str):
    """Get a specific template"""
    if template_name not in templates:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Template not found")
    return templates[template_name]


@app.post("/templates/{template_name}/send")
async def send_from_template(
    template_name: str,
    recipients: List[str],
    variables: Dict[str, str],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Send notification using a template"""
    if template_name not in templates:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Template not found")

    template = templates[template_name]

    # Replace variables in templates
    subject = template.subject_template
    body = template.body_template

    for var, value in variables.items():
        subject = subject.replace(f"{{{{{var}}}}}", value)
        body = body.replace(f"{{{{{var}}}}}", value)

    notification = models.NotificationCreate(
        type=template.type,
        title=subject,
        message=body,
        recipients=recipients,
        channels=[models.NotificationChannel.IN_APP, models.NotificationChannel.EMAIL],
    )

    return await create_notification(notification, user_id, db_session)


# --- Workflow Notification Helpers ---
@app.post("/workflow/{workflow_id}/notify")
async def notify_workflow_event(
    workflow_id: str,
    event_type: Literal["started", "completed", "failed", "cancelled"],
    participants: List[str],
    metadata: Optional[Dict[str, Any]] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Send workflow-related notifications"""
    event_messages = {
        "started": ("Workflow Started", f"Workflow {workflow_id} has been initiated."),
        "completed": ("Workflow Completed", f"Workflow {workflow_id} has been completed successfully."),
        "failed": ("Workflow Failed", f"Workflow {workflow_id} has failed."),
        "cancelled": ("Workflow Cancelled", f"Workflow {workflow_id} has been cancelled."),
    }

    title, message = event_messages.get(event_type, ("Workflow Event", f"Workflow {workflow_id} update"))

    notification = models.NotificationCreate(
        type=(
            models.NotificationType.WORKFLOW_COMPLETED
            if event_type == "completed"
            else models.NotificationType.WORKFLOW_FAILED
        ),
        title=title,
        message=message,
        recipients=participants,
        channels=[models.NotificationChannel.IN_APP],
        metadata={"workflow_id": workflow_id, "event_type": event_type, **(metadata or {})},
        action_url=f"/workflows/{workflow_id}",
    )

    return await create_notification(notification, user_id, db_session)


# --- Approval Notification Helpers ---
@app.post("/approval/{approval_id}/notify")
async def notify_approval_event(
    approval_id: str,
    event_type: Literal["requested", "approved", "rejected", "commented"],
    requester: str,
    approvers: List[str],
    metadata: Optional[Dict[str, Any]] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Send approval-related notifications"""
    notification_map = {
        "requested": (
            models.NotificationType.APPROVAL_REQUIRED,
            "Approval Required",
            f"New approval request {approval_id}",
        ),
        "approved": (
            models.NotificationType.APPROVAL_COMPLETED,
            "Request Approved",
            f"Your request {approval_id} has been approved",
        ),
        "rejected": (
            models.NotificationType.APPROVAL_REJECTED,
            "Request Rejected",
            f"Your request {approval_id} has been rejected",
        ),
        "commented": (
            models.NotificationType.COMMENT_ADDED,
            "Comment Added",
            f"New comment on approval {approval_id}",
        ),
    }

    notif_type, title, message = notification_map.get(
        event_type, (models.NotificationType.SYSTEM, "Approval Update", "")
    )

    notification = models.NotificationCreate(
        type=notif_type,
        title=title,
        message=message,
        priority=models.NotificationPriority.HIGH if event_type == "requested" else models.NotificationPriority.NORMAL,
        recipients=approvers if event_type == "requested" else [requester],
        channels=[models.NotificationChannel.IN_APP, models.NotificationChannel.EMAIL],
        metadata={"approval_id": approval_id, "requester": requester, **(metadata or {})},
        action_url=f"/approvals/{approval_id}",
    )

    return await create_notification(notification, user_id, db_session)


# --- Stats Endpoint (caller's own data) ---
@app.get("/stats")
async def get_notification_stats(
    caller: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get notification statistics for the caller (previously global across all users)."""
    own = await crud.list_notifications(db_session, caller, limit=200)
    unread = sum(1 for n in own if n.status != "read")

    return {
        "total_notifications": len(own),
        "unread_notifications": unread,
        "active_connections": sum(len(conns) for conns in notification_manager.active_connections.values()),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=int(os.getenv("PORT", "8091")))
