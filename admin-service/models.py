"""Pydantic models for Admin Service (API contract unchanged)."""

import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field


class FeatureCategory(str, Enum):
    ACCOUNTING = "accounting"
    FINANCE = "finance"
    BANKING = "banking"
    FRAUD_DETECTION = "fraud_detection"
    REPORTING = "reporting"
    WORKFLOW = "workflow"
    MULTIMODAL = "multimodal"
    INTEGRATION = "integration"
    NOTIFICATIONS = "notifications"
    SECURITY = "security"
    SYSTEM = "system"


class FeatureStatus(str, Enum):
    ENABLED = "enabled"
    DISABLED = "disabled"
    BETA = "beta"
    DEPRECATED = "deprecated"


class FeatureRequestStatus(str, Enum):
    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    IMPLEMENTED = "implemented"


class Feature(BaseModel):
    id: str
    name: str
    description: str
    category: FeatureCategory
    status: FeatureStatus
    enabled_by_default: bool
    requires_permission: Optional[str] = None
    config: Optional[Dict[str, Any]] = None
    rollout_percentage: int = 100  # 0-100, for gradual rollouts
    metadata: Optional[Dict[str, Any]] = None


class FeatureUpdate(BaseModel):
    status: Optional[FeatureStatus] = None
    config: Optional[Dict[str, Any]] = None
    rollout_percentage: Optional[int] = None


class SystemConfig(BaseModel):
    key: str
    value: Any
    description: Optional[str] = None
    category: str
    is_sensitive: bool = False
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    updated_by: Optional[str] = None


class AuditLogEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    timestamp: datetime = Field(default_factory=datetime.utcnow)
    user_id: str
    user_email: str
    action: str
    resource_type: str
    resource_id: str
    changes: Optional[Dict[str, Any]] = None
    ip_address: Optional[str] = None
    user_agent: Optional[str] = None


class ServiceHealth(BaseModel):
    service_name: str
    status: Literal["healthy", "degraded", "unhealthy", "unknown"]
    version: Optional[str] = None
    uptime_seconds: Optional[float] = None
    last_check: datetime = Field(default_factory=datetime.utcnow)
    endpoints: Optional[Dict[str, str]] = None
    error_message: Optional[str] = None


# ============================================================================
# Organization Feature Configuration Models
# ============================================================================


class OrgFeatureConfig(BaseModel):
    """Organization-specific feature configuration"""

    organization_id: str
    feature_id: str
    enabled: bool
    custom_config: Optional[Dict[str, Any]] = None
    rollout_percentage: int = 100
    enabled_at: Optional[datetime] = None
    disabled_at: Optional[datetime] = None
    enabled_by: Optional[str] = None
    notes: Optional[str] = None


class FeatureDependency(BaseModel):
    """Feature dependency configuration"""

    feature_id: str
    depends_on: List[str]  # List of feature IDs that must be enabled
    required_permissions: List[str] = []
    min_rollout_percentage: int = 50  # Minimum rollout before this feature can be enabled


class FeatureRolloutSchedule(BaseModel):
    """Scheduled feature rollout"""

    feature_id: str
    organization_id: Optional[str] = None
    scheduled_date: datetime
    target_percentage: int
    status: str = "scheduled"  # scheduled, in_progress, completed, cancelled
    created_by: str
    created_at: datetime = Field(default_factory=datetime.utcnow)


class FeatureRequest(BaseModel):
    """User-submitted feature request"""

    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    user_email: str
    organization_id: Optional[str] = None
    feature_name: str
    feature_description: Optional[str] = None
    category: Optional[FeatureCategory] = None
    priority: str = "normal"  # low, normal, high, urgent
    business_justification: Optional[str] = None
    status: FeatureRequestStatus = FeatureRequestStatus.PENDING
    reviewed_by: Optional[str] = None
    reviewed_at: Optional[datetime] = None
    review_notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


# ============================================================================
# Feature Registry
# ============================================================================
