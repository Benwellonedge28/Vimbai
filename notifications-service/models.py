"""Pydantic models for the Notifications Service (unchanged contracts)."""

from datetime import datetime
from enum import Enum
from typing import Dict, List, Literal, Optional

from pydantic import BaseModel, Field


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
    metadata: Optional[Dict[str, object]] = None
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
