"""
FinAcc Admin Service
Centralized admin interface for feature management, system configuration, and admin controls
Includes organization-level feature settings, rollout schedules, feature dependencies,
and user-requested feature management
"""

from fastapi import FastAPI, HTTPException, Depends, BackgroundTasks, Request
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any, Literal
from datetime import datetime, timezone, timedelta
from enum import Enum
import asyncio
import uuid
import os
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Admin Service",
    description="Admin interface for system configuration, feature management, organization controls, and user feature requests",
    version="1.2.0",
)

# ============================================================================
# Enums and Models
# ============================================================================

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

FEATURES: Dict[str, Feature] = {
    # Accounting Features
    "double_entry": Feature(
        id="double_entry",
        name="Double-Entry Accounting",
        description="Enable double-entry bookkeeping with debit/credit validation",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.double_entry",
    ),
    "single_entry": Feature(
        id="single_entry",
        name="Single-Entry System",
        description="Enable single-entry (incomplete records) accounting",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.single_entry",
    ),
    "fund_accounting": Feature(
        id="fund_accounting",
        name="Fund Accounting",
        description="Enable fund-based accounting for nonprofits/government",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.fund",
    ),
    "project_accounting": Feature(
        id="project_accounting",
        name="Project Accounting",
        description="Enable project/cost center tracking",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.project",
    ),
    "npo_accounting": Feature(
        id="npo_accounting",
        name="NPO Accounting",
        description="Enable nonprofit organization specific features",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.npo",
    ),
    "depreciation_tracking": Feature(
        id="depreciation_tracking",
        name="Fixed Asset Depreciation",
        description="Enable automatic depreciation calculation for fixed assets",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),

    # Finance Features
    "budgeting": Feature(
        id="budgeting",
        name="Budget Management",
        description="Enable budget creation, tracking, and variance analysis",
        category=FeatureCategory.FINANCE,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "scenario_modeling": Feature(
        id="scenario_modeling",
        name="What-If Scenario Modeling",
        description="Enable financial scenario creation and comparison",
        category=FeatureCategory.FINANCE,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "forecasting": Feature(
        id="forecasting",
        name="Cash Flow Forecasting",
        description="Enable AI-assisted cash flow forecasting",
        category=FeatureCategory.FINANCE,
        status=FeatureStatus.BETA,
        enabled_by_default=False,
    ),

    # Banking Features
    "bank_integration": Feature(
        id="bank_integration",
        name="Bank Feed Integration",
        description="Enable automatic bank feed imports and reconciliation",
        category=FeatureCategory.BANKING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "pos_integration": Feature(
        id="pos_integration",
        name="POS Integration",
        description="Enable Point-of-Sale system integration",
        category=FeatureCategory.BANKING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),

    # Fraud Detection
    "fraud_detection": Feature(
        id="fraud_detection",
        name="Real-time Fraud Detection",
        description="Enable ML-based fraud detection on transactions",
        category=FeatureCategory.FRAUD_DETECTION,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "fraud_alerts": Feature(
        id="fraud_alerts",
        name="Fraud Alert Notifications",
        description="Enable real-time fraud alert notifications",
        category=FeatureCategory.FRAUD_DETECTION,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),

    # Reporting
    "custom_reports": Feature(
        id="custom_reports",
        name="Custom Report Builder",
        description="Enable drag-and-drop report builder",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "pdf_export": Feature(
        id="pdf_export",
        name="PDF Export",
        description="Enable PDF export for reports",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "excel_export": Feature(
        id="excel_export",
        name="Excel Export",
        description="Enable Excel export for reports",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "financial_statements": Feature(
        id="financial_statements",
        name="Financial Statement Generation",
        description="Enable automatic income statement, balance sheet, cash flow",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),

    # Workflow
    "approval_workflows": Feature(
        id="approval_workflows",
        name="Approval Workflows",
        description="Enable configurable approval chains",
        category=FeatureCategory.WORKFLOW,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "audit_trail": Feature(
        id="audit_trail",
        name="Immutable Audit Trail",
        description="Track all changes with immutable audit log",
        category=FeatureCategory.WORKFLOW,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),

    # Multimodal
    "ocr_processing": Feature(
        id="ocr_processing",
        name="OCR Document Processing",
        description="Enable OCR for scanned documents",
        category=FeatureCategory.MULTIMODAL,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "voice_input": Feature(
        id="voice_input",
        name="Voice Input",
        description="Enable voice-to-journal-entry feature",
        category=FeatureCategory.MULTIMODAL,
        status=FeatureStatus.BETA,
        enabled_by_default=False,
    ),

    # Security
    "oauth_login": Feature(
        id="oauth_login",
        name="OAuth2/OIDC Login",
        description="Enable social login (Google, GitHub, Microsoft)",
        category=FeatureCategory.SECURITY,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "mfa": Feature(
        id="mfa",
        name="Multi-Factor Authentication",
        description="Enable TOTP-based MFA",
        category=FeatureCategory.SECURITY,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "rate_limiting": Feature(
        id="rate_limiting",
        name="API Rate Limiting",
        description="Enable rate limiting on API endpoints",
        category=FeatureCategory.SECURITY,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),

    # System
    "offline_mode": Feature(
        id="offline_mode",
        name="Offline-First Mode",
        description="Enable offline data entry and sync",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "graphql_api": Feature(
        id="graphql_api",
        name="GraphQL API",
        description="Enable GraphQL API endpoint",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "websocket_alerts": Feature(
        id="websocket_alerts",
        name="Real-time WebSocket Alerts",
        description="Enable WebSocket for real-time notifications",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "multi_currency": Feature(
        id="multi_currency",
        name="Multi-Currency Support",
        description="Enable multi-currency transactions and conversion",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
}

# ============================================================================
# Organization Feature Configurations Store
# ============================================================================

org_feature_configs: Dict[str, OrgFeatureConfig] = {}

# ============================================================================
# Feature Dependencies Store
# ============================================================================

FEATURE_DEPENDENCIES: Dict[str, FeatureDependency] = {
    "forecasting": FeatureDependency(
        feature_id="forecasting",
        depends_on=["budgeting", "scenario_modeling"],
        required_permissions=["finance.forecasting"],
        min_rollout_percentage=50,
    ),
    "voice_input": FeatureDependency(
        feature_id="voice_input",
        depends_on=["ocr_processing"],
        required_permissions=["multimodal.voice"],
        min_rollout_percentage=25,
    ),
    "approval_workflows": FeatureDependency(
        feature_id="approval_workflows",
        depends_on=["audit_trail"],
        required_permissions=["workflow.approval"],
        min_rollout_percentage=10,
    ),
}

# ============================================================================
# Feature Rollout Schedules Store
# ============================================================================

rollout_schedules: List[FeatureRolloutSchedule] = []

# ============================================================================
# Feature Requests Store
# ============================================================================

feature_requests: Dict[str, FeatureRequest] = {}

# ============================================================================
# Configuration Store
# ============================================================================

SYSTEM_CONFIG: Dict[str, SystemConfig] = {
    "company_name": SystemConfig(
        key="company_name",
        value="FinAcc Corporation",
        description="Company name displayed in reports",
        category="general",
    ),
    "fiscal_year_start": SystemConfig(
        key="fiscal_year_start",
        value="January",
        description="Start month of fiscal year",
        category="accounting",
    ),
    "base_currency": SystemConfig(
        key="base_currency",
        value="USD",
        description="Primary currency for financial statements",
        category="accounting",
    ),
    "date_format": SystemConfig(
        key="date_format",
        value="YYYY-MM-DD",
        description="Date format for displays",
        category="general",
    ),
    "timezone": SystemConfig(
        key="timezone",
        value="UTC",
        description="System timezone",
        category="general",
    ),
    "session_timeout_minutes": SystemConfig(
        key="session_timeout_minutes",
        value=60,
        description="Session timeout in minutes",
        category="security",
        is_sensitive=False,
    ),
    "max_login_attempts": SystemConfig(
        key="max_login_attempts",
        value=5,
        description="Maximum failed login attempts before lockout",
        category="security",
    ),
    "maintenance_mode": SystemConfig(
        key="maintenance_mode",
        value=False,
        description="Enable system maintenance mode",
        category="system",
    ),
}

# ============================================================================
# Audit Log Store
# ============================================================================

audit_logs: List[AuditLogEntry] = []

# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "admin",
        "version": "1.2.0",
    }

# --- Feature Management ---

@app.get("/features")
async def list_features(
    category: Optional[FeatureCategory] = None,
    status: Optional[FeatureStatus] = None,
    enabled_only: bool = False
):
    """List all features with optional filtering"""
    result = list(FEATURES.values())

    if category:
        result = [f for f in result if f.category == category]
    if status:
        result = [f for f in result if f.status == status]
    if enabled_only:
        result = [f for f in result if f.status == FeatureStatus.ENABLED]

    return result


@app.get("/features/{feature_id}")
async def get_feature(feature_id: str):
    """Get a specific feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")
    return FEATURES[feature_id]


@app.put("/features/{feature_id}")
async def update_feature(feature_id: str, update: FeatureUpdate):
    """Update a feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    feature = FEATURES[feature_id]

    if update.status:
        feature.status = update.status
    if update.config:
        feature.config = update.config
    if update.rollout_percentage is not None:
        feature.rollout_percentage = update.rollout_percentage

    # Log the change
    audit_logs.append(AuditLogEntry(
        user_id="admin",
        user_email="admin@finacc.com",
        action="feature_updated",
        resource_type="feature",
        resource_id=feature_id,
        changes={"status": update.status, "config": update.config}
    ))

    return feature


@app.post("/features/{feature_id}/enable")
async def enable_feature(feature_id: str):
    """Enable a feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    FEATURES[feature_id].status = FeatureStatus.ENABLED
    return {"status": "enabled", "feature_id": feature_id}


@app.post("/features/{feature_id}/disable")
async def disable_feature(feature_id: str):
    """Disable a feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    FEATURES[feature_id].status = FeatureStatus.DISABLED
    return {"status": "disabled", "feature_id": feature_id}


@app.get("/features/categories")
async def list_feature_categories():
    """List all feature categories"""
    return [
        {"name": cat.name, "value": cat.value}
        for cat in FeatureCategory
    ]


# --- Organization Feature Configuration ---

@app.get("/organizations/{organization_id}/features")
async def get_org_features(organization_id: str):
    """Get all feature configurations for an organization"""
    org_configs = {
        f.feature_id: f
        for f in org_feature_configs.values()
        if f.organization_id == organization_id
    }

    # Merge with default features
    result = []
    for feature_id, feature in FEATURES.items():
        if feature_id in org_configs:
            org_config = org_configs[feature_id]
            result.append({
                **feature.model_dump(),
                "org_enabled": org_config.enabled,
                "org_rollout_percentage": org_config.rollout_percentage,
                "org_custom_config": org_config.custom_config,
            })
        else:
            result.append({
                **feature.model_dump(),
                "org_enabled": feature.enabled_by_default,
                "org_rollout_percentage": feature.rollout_percentage,
                "org_custom_config": None,
            })

    return result


@app.put("/organizations/{organization_id}/features/{feature_id}")
async def update_org_feature(
    organization_id: str,
    feature_id: str,
    enabled: bool,
    custom_config: Optional[Dict[str, Any]] = None,
    rollout_percentage: int = 100,
    notes: Optional[str] = None,
    updated_by: str = "admin"
):
    """Update organization-specific feature configuration"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    config_key = f"{organization_id}:{feature_id}"
    now = datetime.utcnow()

    if config_key in org_feature_configs:
        config = org_feature_configs[config_key]
        config.enabled = enabled
        config.custom_config = custom_config
        config.rollout_percentage = rollout_percentage
        config.notes = notes
        if enabled and not config.enabled_at:
            config.enabled_at = now
        if not enabled:
            config.disabled_at = now
        config.enabled_by = updated_by
    else:
        config = OrgFeatureConfig(
            organization_id=organization_id,
            feature_id=feature_id,
            enabled=enabled,
            custom_config=custom_config,
            rollout_percentage=rollout_percentage,
            enabled_at=now if enabled else None,
            disabled_at=None if enabled else now,
            enabled_by=updated_by,
            notes=notes,
        )
        org_feature_configs[config_key] = config

    # Log the change
    audit_logs.append(AuditLogEntry(
        user_id=updated_by,
        user_email=f"{updated_by}@finacc.com",
        action="org_feature_updated",
        resource_type="org_feature",
        resource_id=config_key,
        changes={
            "enabled": enabled,
            "rollout_percentage": rollout_percentage,
            "organization_id": organization_id
        }
    ))

    return config


@app.post("/organizations/{organization_id}/features/{feature_id}/enable")
async def enable_org_feature(
    organization_id: str,
    feature_id: str,
    updated_by: str = "admin"
):
    """Enable a feature for a specific organization"""
    return await update_org_feature(
        organization_id, feature_id, True, None, 100, None, updated_by
    )


@app.post("/organizations/{organization_id}/features/{feature_id}/disable")
async def disable_org_feature(
    organization_id: str,
    feature_id: str,
    updated_by: str = "admin"
):
    """Disable a feature for a specific organization"""
    return await update_org_feature(
        organization_id, feature_id, False, None, 0, None, updated_by
    )


# --- Feature Dependencies ---

@app.get("/features/{feature_id}/dependencies")
async def get_feature_dependencies(feature_id: str):
    """Get dependencies for a feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    dependency = FEATURE_DEPENDENCIES.get(feature_id)
    if not dependency:
        return {"feature_id": feature_id, "dependencies": [], "satisfied": True}

    # Check if dependencies are satisfied
    satisfied = True
    missing_deps = []
    for dep_id in dependency.depends_on:
        if dep_id in FEATURES:
            dep_feature = FEATURES[dep_id]
            if dep_feature.status != FeatureStatus.ENABLED:
                satisfied = False
                missing_deps.append(dep_id)
        else:
            satisfied = False
            missing_deps.append(dep_id)

    return {
        "feature_id": feature_id,
        "depends_on": dependency.depends_on,
        "required_permissions": dependency.required_permissions,
        "min_rollout_percentage": dependency.min_rollout_percentage,
        "satisfied": satisfied,
        "missing_dependencies": missing_deps,
    }


# --- Feature Rollout Schedules ---

@app.get("/rollout-schedules")
async def list_rollout_schedules(
    feature_id: Optional[str] = None,
    organization_id: Optional[str] = None,
    status: Optional[str] = None
):
    """List all feature rollout schedules"""
    result = rollout_schedules

    if feature_id:
        result = [s for s in result if s.feature_id == feature_id]
    if organization_id:
        result = [s for s in result if s.organization_id == organization_id]
    if status:
        result = [s for s in result if s.status == status]

    return result


@app.post("/rollout-schedules")
async def create_rollout_schedule(
    feature_id: str,
    scheduled_date: datetime,
    target_percentage: int,
    organization_id: Optional[str] = None,
    created_by: str = "admin"
):
    """Schedule a feature rollout"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    schedule = FeatureRolloutSchedule(
        feature_id=feature_id,
        organization_id=organization_id,
        scheduled_date=scheduled_date,
        target_percentage=target_percentage,
        created_by=created_by,
    )
    rollout_schedules.append(schedule)

    # Log the change
    audit_logs.append(AuditLogEntry(
        user_id=created_by,
        user_email=f"{created_by}@finacc.com",
        action="rollout_scheduled",
        resource_type="rollout_schedule",
        resource_id=feature_id,
        changes={"scheduled_date": scheduled_date, "target_percentage": target_percentage}
    ))

    return schedule


@app.delete("/rollout-schedules/{schedule_id}")
async def cancel_rollout_schedule(schedule_id: str):
    """Cancel a scheduled rollout"""
    for schedule in rollout_schedules:
        if schedule.feature_id == schedule_id or schedule.id == schedule_id:
            schedule.status = "cancelled"
            return {"status": "cancelled", "schedule_id": schedule.id}

    raise HTTPException(status_code=404, detail="Schedule not found")


# --- Feature Requests (User-Requested Features) ---

@app.post("/feature-requests")
async def create_feature_request(
    user_id: str,
    user_email: str,
    feature_name: str,
    organization_id: Optional[str] = None,
    feature_description: Optional[str] = None,
    category: Optional[FeatureCategory] = None,
    priority: str = "normal",
    business_justification: Optional[str] = None
):
    """Submit a new feature request"""
    request = FeatureRequest(
        user_id=user_id,
        user_email=user_email,
        organization_id=organization_id,
        feature_name=feature_name,
        feature_description=feature_description,
        category=category,
        priority=priority,
        business_justification=business_justification,
    )
    feature_requests[request.id] = request

    # Log the request
    audit_logs.append(AuditLogEntry(
        user_id=user_id,
        user_email=user_email,
        action="feature_request_submitted",
        resource_type="feature_request",
        resource_id=request.id,
        changes={"feature_name": feature_name, "priority": priority}
    ))

    return request


@app.get("/feature-requests")
async def list_feature_requests(
    status: Optional[FeatureRequestStatus] = None,
    organization_id: Optional[str] = None,
    priority: Optional[str] = None,
    limit: int = 50
):
    """List all feature requests with filters"""
    result = list(feature_requests.values())

    if status:
        result = [r for r in result if r.status == status]
    if organization_id:
        result = [r for r in result if r.organization_id == organization_id]
    if priority:
        result = [r for r in result if r.priority == priority]

    result.sort(key=lambda x: x.created_at, reverse=True)
    return result[:limit]


@app.get("/feature-requests/{request_id}")
async def get_feature_request(request_id: str):
    """Get a specific feature request"""
    if request_id not in feature_requests:
        raise HTTPException(status_code=404, detail="Feature request not found")
    return feature_requests[request_id]


@app.put("/feature-requests/{request_id}/review")
async def review_feature_request(
    request_id: str,
    status: FeatureRequestStatus,
    reviewed_by: str,
    review_notes: Optional[str] = None
):
    """Review and update a feature request status"""
    if request_id not in feature_requests:
        raise HTTPException(status_code=404, detail="Feature request not found")

    request = feature_requests[request_id]
    request.status = status
    request.reviewed_by = reviewed_by
    request.reviewed_at = datetime.utcnow()
    request.review_notes = review_notes
    request.updated_at = datetime.utcnow()

    # Log the review
    audit_logs.append(AuditLogEntry(
        user_id=reviewed_by,
        user_email=f"{reviewed_by}@finacc.com",
        action="feature_request_reviewed",
        resource_type="feature_request",
        resource_id=request_id,
        changes={"status": status.value, "review_notes": review_notes}
    ))

    return request


@app.delete("/feature-requests/{request_id}")
async def delete_feature_request(request_id: str):
    """Delete a feature request"""
    if request_id not in feature_requests:
        raise HTTPException(status_code=404, detail="Feature request not found")

    del feature_requests[request_id]
    return {"status": "deleted", "request_id": request_id}


# --- System Configuration ---

@app.get("/config")
async def list_config(category: Optional[str] = None):
    """List system configuration"""
    result = list(SYSTEM_CONFIG.values())

    if category:
        result = [c for c in result if c.category == category]

    # Mask sensitive values
    masked_result = []
    for config in result:
        if config.is_sensitive and config.value:
            config_dict = config.model_dump()
            config_dict["value"] = "***HIDDEN***"
            masked_result.append(config_dict)
        else:
            masked_result.append(config.model_dump())

    return masked_result


@app.get("/config/{key}")
async def get_config(key: str, include_sensitive: bool = False):
    """Get a specific configuration value"""
    if key not in SYSTEM_CONFIG:
        raise HTTPException(status_code=404, detail="Configuration not found")

    config = SYSTEM_CONFIG[key]

    if config.is_sensitive and not include_sensitive:
        return {
            "key": config.key,
            "value": "***HIDDEN***",
            "description": config.description,
            "category": config.category,
            "is_sensitive": True,
        }

    return config


@app.put("/config/{key}")
async def update_config(key: str, value: Any, updated_by: str = "admin"):
    """Update a configuration value"""
    if key not in SYSTEM_CONFIG:
        raise HTTPException(status_code=404, detail="Configuration not found")

    config = SYSTEM_CONFIG[key]
    old_value = config.value
    config.value = value
    config.updated_at = datetime.utcnow()
    config.updated_by = updated_by

    # Log the change
    audit_logs.append(AuditLogEntry(
        user_id=updated_by,
        user_email=f"{updated_by}@finacc.com",
        action="config_updated",
        resource_type="config",
        resource_id=key,
        changes={"old_value": old_value, "new_value": value}
    ))

    return config


# --- Audit Logs ---

@app.get("/audit-logs")
async def list_audit_logs(
    user_id: Optional[str] = None,
    action: Optional[str] = None,
    resource_type: Optional[str] = None,
    limit: int = 100
):
    """List audit log entries"""
    result = audit_logs

    if user_id:
        result = [e for e in result if e.user_id == user_id]
    if action:
        result = [e for e in result if e.action == action]
    if resource_type:
        result = [e for e in result if e.resource_type == resource_type]

    result.sort(key=lambda x: x.timestamp, reverse=True)
    return result[:limit]


@app.post("/audit-logs")
async def create_audit_entry(
    user_id: str,
    user_email: str,
    action: str,
    resource_type: str,
    resource_id: str,
    changes: Optional[Dict[str, Any]] = None
):
    """Create an audit log entry"""
    entry = AuditLogEntry(
        user_id=user_id,
        user_email=user_email,
        action=action,
        resource_type=resource_type,
        resource_id=resource_id,
        changes=changes,
    )
    audit_logs.append(entry)

    # Keep only last 10000 entries
    if len(audit_logs) > 10000:
        audit_logs.pop(0)

    return entry


# --- Service Health ---

@app.get("/services/health")
async def get_services_health():
    """Get health status of all microservices"""
    services = [
        ServiceHealth(
            service_name="accounting-service",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8000"},
        ),
        ServiceHealth(
            service_name="finance-service",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8001"},
        ),
        ServiceHealth(
            service_name="identity-service",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8080"},
        ),
        ServiceHealth(
            service_name="audit-service",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8091"},
        ),
        ServiceHealth(
            service_name="api-gateway",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8081"},
        ),
        ServiceHealth(
            service_name="alerts-service",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8090"},
        ),
        ServiceHealth(
            service_name="notifications-service",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8091"},
        ),
        ServiceHealth(
            service_name="message-bus-service",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8097"},
        ),
        ServiceHealth(
            service_name="automation-engine",
            status="healthy",
            version="1.0.0",
            endpoints={"api": "http://localhost:8098"},
        ),
    ]
    return services


# --- Dashboard Stats ---

@app.get("/dashboard/stats")
async def get_dashboard_stats():
    """Get admin dashboard statistics"""
    enabled_features = sum(1 for f in FEATURES.values() if f.status == FeatureStatus.ENABLED)
    beta_features = sum(1 for f in FEATURES.values() if f.status == FeatureStatus.BETA)
    disabled_features = sum(1 for f in FEATURES.values() if f.status == FeatureStatus.DISABLED)

    pending_requests = sum(1 for r in feature_requests.values() if r.status == FeatureRequestStatus.PENDING)
    approved_requests = sum(1 for r in feature_requests.values() if r.status == FeatureRequestStatus.APPROVED)

    scheduled_rollouts = sum(1 for s in rollout_schedules if s.status == "scheduled")
    active_org_configs = len(set(f"{c.organization_id}:{c.feature_id}" for c in org_feature_configs.values()))

    return {
        "total_features": len(FEATURES),
        "enabled_features": enabled_features,
        "beta_features": beta_features,
        "disabled_features": disabled_features,
        "total_config_entries": len(SYSTEM_CONFIG),
        "audit_logs_count": len(audit_logs),
        "feature_requests": {
            "pending": pending_requests,
            "approved": approved_requests,
            "total": len(feature_requests),
        },
        "rollout_schedules": {
            "scheduled": scheduled_rollouts,
            "total": len(rollout_schedules),
        },
        "organization_configs": active_org_configs,
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8099)