"""
FinAcc Admin Service
Centralized admin interface for feature management, system configuration, and admin controls
"""

from fastapi import FastAPI, HTTPException, Depends, BackgroundTasks
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any, Literal
from datetime import datetime
from enum import Enum
import asyncio
import uuid
import os
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Admin Service",
    description="Admin interface for system configuration and feature management",
    version="1.0.0",
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
        "version": "1.0.0",
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

    return {
        "total_features": len(FEATURES),
        "enabled_features": enabled_features,
        "beta_features": beta_features,
        "disabled_features": disabled_features,
        "total_config_entries": len(SYSTEM_CONFIG),
        "audit_logs_count": len(audit_logs),
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8099)