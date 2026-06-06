"""
FinAcc Notification Service
Handles notifications for workflows, approvals, and system events
"""

from fastapi import FastAPI, WebSocket, WebSocketDisconnect, HTTPException, BackgroundTasks, Depends, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any, Literal
from datetime import datetime, timedelta
from enum import Enum
import asyncio
import json
import uuid
import os
from collections import defaultdict
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Notifications Service",
    description="Notification and messaging system for FinAcc workflows",
    version="0.1.0",
)

# ============================================================================
# Models
# ============================================================================

class NotificationType(str, Enum):
    APPROVAL_REQUIRED = "approval_required"
    APPROVAL_COMPLETED = "approval_completed"
    APPROVAL_REJECTED = "approval_rejected"
    COMMENT_ADDED = "comment_added"
    MENTION = "mention"
    WORKFLOW_COMPLETED = "workflow_completed"
    WORKFLOW_FAILED = "workflow_failed"
    DEADLINE_REMINDER = "deadline_reminder"
    SYSTEM = "system"

class NotificationPriority(str, Enum):
    URGENT = "urgent"
    HIGH = "high"
    NORMAL = "normal"
    LOW = "low"

class NotificationChannel(str, Enum):
    IN_APP = "in_app"
    EMAIL = "email"
    SMS = "sms"
    WEBHOOK = "webhook"
    PUSH = "push"

class NotificationCreate(BaseModel):
    type: NotificationType
    title: str
    message: str
    priority: NotificationPriority = NotificationPriority.NORMAL
    recipients: List[str] = Field(..., min_items=1)
    channels: List[NotificationChannel] = [NotificationChannel.IN_APP]
    metadata: Optional[Dict[str, Any]] = None
    action_url: Optional[str] = None
    scheduled_at: Optional[datetime] = None
    expires_at: Optional[datetime] = None

class NotificationInDB(NotificationCreate):
    id: str
    sender: Optional[str] = None
    status: Literal["pending", "sent", "failed", "read", "archived"] = "pending"
    created_at: datetime
    sent_at: Optional[datetime] = None
    read_at: Optional[datetime] = None

class NotificationPreferences(BaseModel):
    user_id: str
    channels: Dict[NotificationType, List[NotificationChannel]] = {}
    quiet_hours_start: Optional[str] = None
    quiet_hours_end: Optional[str] = None
    email_batch: bool = True
    email_batch_interval_minutes: int = 60

class NotificationTemplate(BaseModel):
    name: str
    type: NotificationType
    subject_template: str
    body_template: str
    variables: List[str] = []

# ============================================================================
# WebSocket Connection Manager
# ============================================================================

class NotificationManager:
    """Manages notification connections and delivery"""

    def __init__(self):
        self.active_connections: Dict[str, List[WebSocket]] = defaultdict(list)
        self.user_notifications: Dict[str, List[NotificationInDB]] = defaultdict(list)
        self.lock = asyncio.Lock()
        self.pending_batches: Dict[str, List[NotificationInDB]] = defaultdict(list)

    async def connect(self, websocket: WebSocket, user_id: str):
        await websocket.accept()
        async with self.lock:
            self.active_connections[user_id].append(websocket)
            # Send unread notifications on connect
            unread = [n for n in self.user_notifications.get(user_id, []) if n.status != "read"]
            if unread:
                await websocket.send_json({
                    "type": "unread_count",
                    "count": len(unread)
                })

    async def disconnect(self, websocket: WebSocket, user_id: str):
        async with self.lock:
            if user_id in self.active_connections:
                try:
                    self.active_connections[user_id].remove(websocket)
                except ValueError:
                    pass
                if not self.active_connections[user_id]:
                    del self.active_connections[user_id]

    async def send_notification(self, notification: NotificationInDB, user_id: str):
        """Send notification to user via all available channels"""
        async with self.lock:
            self.user_notifications[user_id].append(notification)

        # Send via WebSocket if connected
        await self._send_websocket(user_id, {
            "type": "notification",
            "notification": self._serialize_notification(notification)
        })

        # Process additional channels
        for channel in notification.channels:
            await self._send_via_channel(notification, user_id, channel)

    async def _send_websocket(self, user_id: str, message: dict):
        """Send message via WebSocket"""
        if user_id in self.active_connections:
            disconnected = []
            for ws in self.active_connections[user_id]:
                try:
                    await ws.send_json(message)
                except Exception:
                    disconnected.append(ws)

            # Clean up
            async with self.lock:
                for ws in disconnected:
                    try:
                        self.active_connections[user_id].remove(ws)
                    except ValueError:
                        pass

    async def _send_via_channel(self, notification: NotificationInDB, user_id: str, channel: NotificationChannel):
        """Send notification via specific channel"""
        if channel == NotificationChannel.EMAIL:
            await self._send_email(notification, user_id)
        elif channel == NotificationChannel.WEBHOOK:
            await self._send_webhook(notification, user_id)
        elif channel == NotificationChannel.PUSH:
            await self._send_push(notification, user_id)
        elif channel == NotificationChannel.SMS:
            await self._send_sms(notification, user_id)

    async def _send_email(self, notification: NotificationInDB, user_id: str):
        """Send email notification (placeholder - integrate with email service)"""
        # In production, integrate with email service (SendGrid, SES, etc.)
        print(f"Email to {user_id}: {notification.title}")

    async def _send_webhook(self, notification: NotificationInDB, user_id: str):
        """Send webhook notification"""
        # In production, make HTTP request to user's webhook URL
        print(f"Webhook to {user_id}: {notification.title}")

    async def _send_push(self, notification: NotificationInDB, user_id: str):
        """Send push notification"""
        # In production, integrate with push notification service (FCM, APNS)
        print(f"Push to {user_id}: {notification.title}")

    async def _send_sms(self, notification: NotificationInDB, user_id: str):
        """Send SMS notification"""
        # In production, integrate with SMS service (Twilio, etc.)
        print(f"SMS to {user_id}: {notification.message[:50]}")

    def _serialize_notification(self, notification: NotificationInDB) -> dict:
        return {
            "id": notification.id,
            "type": notification.type.value,
            "title": notification.title,
            "message": notification.message,
            "priority": notification.priority.value,
            "metadata": notification.metadata,
            "action_url": notification.action_url,
            "created_at": notification.created_at.isoformat(),
            "status": notification.status
        }

    def get_user_notifications(
        self,
        user_id: str,
        unread_only: bool = False,
        limit: int = 50
    ) -> List[NotificationInDB]:
        """Get notifications for a user"""
        notifications = self.user_notifications.get(user_id, [])
        if unread_only:
            notifications = [n for n in notifications if n.status != "read"]
        return sorted(notifications, key=lambda x: x.created_at, reverse=True)[:limit]

notification_manager = NotificationManager()

# ============================================================================
# Notification Templates
# ============================================================================

templates = {
    "approval_request": NotificationTemplate(
        name="Approval Request",
        type=NotificationType.APPROVAL_REQUIRED,
        subject_template="Approval Required: {{title}}",
        body_template="You have a new approval request for '{{title}}' from {{sender}}. Please review and take action.",
        variables=["title", "sender", "action_url"]
    ),
    "approval_completed": NotificationTemplate(
        name="Approval Completed",
        type=NotificationType.APPROVAL_COMPLETED,
        subject_template="Approved: {{title}}",
        body_template="Your request '{{title}}' has been approved by {{approver}}.",
        variables=["title", "approver"]
    ),
    "comment_added": NotificationTemplate(
        name="Comment Added",
        type=NotificationType.COMMENT_ADDED,
        subject_template="{{sender}} commented on {{title}}",
        body_template="{{sender}} added a comment: {{comment}}",
        variables=["sender", "title", "comment"]
    ),
    "mention": NotificationTemplate(
        name="Mention",
        type=NotificationType.MENTION,
        subject_template="{{sender}} mentioned you",
        body_template="{{sender}} mentioned you in '{{title}}': {{comment}}",
        variables=["sender", "title", "comment"]
    ),
    "deadline_reminder": NotificationTemplate(
        name="Deadline Reminder",
        type=NotificationType.DEADLINE_REMINDER,
        subject_template="Deadline Reminder: {{title}}",
        body_template="Reminder: '{{title}}' is due on {{deadline}}.",
        variables=["title", "deadline"]
    ),
}

# ============================================================================
# API Endpoints
# ============================================================================

@app.on_event("startup")
async def startup():
    print("Notifications service started")

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "notifications"}

# --- WebSocket Endpoint ---
@app.websocket("/ws/notifications/{user_id}")
async def websocket_notifications(websocket: WebSocket, user_id: str):
    """WebSocket endpoint for real-time notifications"""
    await notification_manager.connect(websocket, user_id)
    try:
        while True:
            data = await websocket.receive_text()
            try:
                message = json.loads(data)
                if message.get("type") == "mark_read":
                    # Mark notification as read
                    notification_id = message.get("notification_id")
                    for notifications in notification_manager.user_notifications.values():
                        for notif in notifications:
                            if notif.id == notification_id:
                                notif.status = "read"
                                notif.read_at = datetime.utcnow()
                                break
                elif message.get("type") == "mark_all_read":
                    for notif in notification_manager.user_notifications.get(user_id, []):
                        if notif.status != "read":
                            notif.status = "read"
                            notif.read_at = datetime.utcnow()
            except json.JSONDecodeError:
                pass
    except WebSocketDisconnect:
        await notification_manager.disconnect(websocket, user_id)

# --- Send Notification ---
@app.post("/notifications", response_model=NotificationInDB, status_code=status.HTTP_201_CREATED)
async def create_notification(
    notification: NotificationCreate,
    sender: Optional[str] = None
):
    """Create and send a notification"""
    notification_id = str(uuid.uuid4())
    now = datetime.utcnow()

    db_notification = NotificationInDB(
        id=notification_id,
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
        sender=sender,
        status="pending",
        created_at=now
    )

    # Send to all recipients
    for recipient in notification.recipients:
        await notification_manager.send_notification(db_notification, recipient)

    db_notification.status = "sent"
    db_notification.sent_at = now

    return db_notification

# --- Send Batch Notifications ---
@app.post("/notifications/batch", status_code=status.HTTP_201_CREATED)
async def create_batch_notifications(
    notifications: List[NotificationCreate],
    sender: Optional[str] = None
):
    """Create and send multiple notifications"""
    results = []
    now = datetime.utcnow()

    for notification in notifications:
        notification_id = str(uuid.uuid4())
        db_notification = NotificationInDB(
            id=notification_id,
            type=notification.type,
            title=notification.title,
            message=notification.message,
            priority=notification.priority,
            recipients=notification.recipients,
            channels=notification.channels,
            metadata=notification.metadata,
            action_url=notification.action_url,
            sender=sender,
            status="pending",
            created_at=now
        )

        for recipient in notification.recipients:
            await notification_manager.send_notification(db_notification, recipient)

        db_notification.status = "sent"
        db_notification.sent_at = now
        results.append(db_notification)

    return {"count": len(results), "notifications": results}

# --- Get User Notifications ---
@app.get("/notifications/{user_id}")
async def get_user_notifications(
    user_id: str,
    unread_only: bool = Query(False),
    limit: int = Query(50, ge=1, le=200)
):
    """Get notifications for a user"""
    notifications = notification_manager.get_user_notifications(user_id, unread_only, limit)
    unread_count = sum(1 for n in notification_manager.user_notifications.get(user_id, []) if n.status != "read")

    return {
        "notifications": notifications,
        "unread_count": unread_count,
        "total_count": len(notification_manager.user_notifications.get(user_id, []))
    }

# --- Mark as Read ---
@app.put("/notifications/{notification_id}/read")
async def mark_notification_read(notification_id: str, user_id: str):
    """Mark a notification as read"""
    for notif in notification_manager.user_notifications.get(user_id, []):
        if notif.id == notification_id:
            notif.status = "read"
            notif.read_at = datetime.utcnow()
            return {"status": "success", "notification_id": notification_id}

    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Notification not found")

@app.put("/notifications/{user_id}/read-all")
async def mark_all_notifications_read(user_id: str):
    """Mark all notifications as read for a user"""
    count = 0
    for notif in notification_manager.user_notifications.get(user_id, []):
        if notif.status != "read":
            notif.status = "read"
            notif.read_at = datetime.utcnow()
            count += 1

    return {"status": "success", "marked_count": count}

# --- Delete Notification ---
@app.delete("/notifications/{notification_id}")
async def delete_notification(notification_id: str, user_id: str):
    """Delete a notification"""
    user_notifs = notification_manager.user_notifications.get(user_id, [])
    for i, notif in enumerate(user_notifs):
        if notif.id == notification_id:
            user_notifs.pop(i)
            return {"status": "success"}

    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Notification not found")

# --- Notification Preferences ---
@app.put("/preferences/{user_id}")
async def update_preferences(user_id: str, preferences: NotificationPreferences):
    """Update notification preferences for a user"""
    # In production, store in database
    return {"status": "success", "preferences": preferences}

@app.get("/preferences/{user_id}")
async def get_preferences(user_id: str):
    """Get notification preferences for a user"""
    return NotificationPreferences(
        user_id=user_id,
        email_batch=True,
        email_batch_interval_minutes=60
    )

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
    sender: Optional[str] = None
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

    notification = NotificationCreate(
        type=template.type,
        title=subject,
        message=body,
        recipients=recipients,
        channels=[NotificationChannel.IN_APP, NotificationChannel.EMAIL]
    )

    return await create_notification(notification, sender)

# --- Workflow Notification Helpers ---
@app.post("/workflow/{workflow_id}/notify")
async def notify_workflow_event(
    workflow_id: str,
    event_type: Literal["started", "completed", "failed", "cancelled"],
    participants: List[str],
    metadata: Optional[Dict[str, Any]] = None
):
    """Send workflow-related notifications"""
    event_messages = {
        "started": ("Workflow Started", f"Workflow {workflow_id} has been initiated."),
        "completed": ("Workflow Completed", f"Workflow {workflow_id} has been completed successfully."),
        "failed": ("Workflow Failed", f"Workflow {workflow_id} has failed."),
        "cancelled": ("Workflow Cancelled", f"Workflow {workflow_id} has been cancelled.")
    }

    title, message = event_messages.get(event_type, ("Workflow Event", f"Workflow {workflow_id} update"))

    notification = NotificationCreate(
        type=NotificationType.WORKFLOW_COMPLETED if event_type == "completed" else NotificationType.WORKFLOW_FAILED,
        title=title,
        message=message,
        recipients=participants,
        channels=[NotificationChannel.IN_APP],
        metadata={"workflow_id": workflow_id, "event_type": event_type, **(metadata or {})},
        action_url=f"/workflows/{workflow_id}"
    )

    return await create_notification(notification)

# --- Approval Notification Helpers ---
@app.post("/approval/{approval_id}/notify")
async def notify_approval_event(
    approval_id: str,
    event_type: Literal["requested", "approved", "rejected", "commented"],
    requester: str,
    approvers: List[str],
    metadata: Optional[Dict[str, Any]] = None
):
    """Send approval-related notifications"""
    notification_map = {
        "requested": (NotificationType.APPROVAL_REQUIRED, "Approval Required", f"New approval request {approval_id}"),
        "approved": (NotificationType.APPROVAL_COMPLETED, "Request Approved", f"Your request {approval_id} has been approved"),
        "rejected": (NotificationType.APPROVAL_REJECTED, "Request Rejected", f"Your request {approval_id} has been rejected"),
        "commented": (NotificationType.COMMENT_ADDED, "Comment Added", f"New comment on approval {approval_id}")
    }

    notif_type, title, message = notification_map.get(event_type, (NotificationType.SYSTEM, "Approval Update", ""))

    notification = NotificationCreate(
        type=notif_type,
        title=title,
        message=message,
        priority=NotificationPriority.HIGH if event_type == "requested" else NotificationPriority.NORMAL,
        recipients=approvers if event_type == "requested" else [requester],
        channels=[NotificationChannel.IN_APP, NotificationChannel.EMAIL],
        metadata={"approval_id": approval_id, "requester": requester, **(metadata or {})},
        action_url=f"/approvals/{approval_id}"
    )

    return await create_notification(notification)

# --- Stats Endpoint ---
@app.get("/stats")
async def get_notification_stats():
    """Get notification statistics"""
    total_notifications = sum(len(n) for n in notification_manager.user_notifications.values())
    unread_total = sum(
        1 for notifs in notification_manager.user_notifications.values()
        for n in notifs if n.status != "read"
    )

    return {
        "total_notifications": total_notifications,
        "unread_notifications": unread_total,
        "active_connections": sum(len(conns) for conns in notification_manager.active_connections.values()),
        "users_with_notifications": len(notification_manager.user_notifications)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8091)