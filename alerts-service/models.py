"""Pydantic models and enums for the Alerts Service."""

from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field


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
