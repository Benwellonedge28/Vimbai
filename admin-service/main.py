"""
Vimbai Admin Service
Feature flags, org feature configs, rollout schedules, feature requests,
system configuration, audit logs and dashboard stats.

Caller-specific state persists in Neo4j, stamped with the caller
(X-User-Id) and the Book context (X-Book-ID, verified upstream by the API
gateway). The code-defined catalogs (feature registry, dependencies,
config defaults, service health) are immutable seeds; per-caller
overrides/org-configs/schedules/requests/audit entries are persisted and
lookups resolve only against the caller's own Book-visible records.
Previously the shared module-level stores let any caller toggle global
feature flags, reconfigure any org, cancel anyone's rollout schedule, and
read the global audit trail.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "admin_service" not in _sys.modules or not hasattr(_sys.modules.get("admin_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("admin_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["admin_service"] = _pkg
    _sys.modules["admin_service"].__path__ = [_HERE]

from admin_service import crud
from admin_service.catalog import FEATURE_DEPENDENCIES, FEATURES, SERVICES_HEALTH, SYSTEM_CONFIG
from admin_service.database import Neo4jConnector
from admin_service.dependencies import book_id_var, get_db_session, get_user_id
from admin_service.models import (
    AuditLogEntry,
    Feature,
    FeatureCategory,
    FeatureRequest,
    FeatureRequestStatus,
    FeatureRolloutSchedule,
    FeatureStatus,
    FeatureUpdate,
    OrgFeatureConfig,
    SystemConfig,
)
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

app = FastAPI(title="Vimbai Admin Service", version="1.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


async def _merged_features(db_session: AsyncSession, caller_id: str) -> Dict[str, Feature]:
    """Catalog features with the caller's Book-visible status/config/rollout overrides applied."""
    overrides = await crud.get_feature_overrides(db_session, caller_id)
    merged: Dict[str, Feature] = {}
    for fid, feature in FEATURES.items():
        ov = overrides.get(fid)
        if ov:
            merged[fid] = feature.model_copy(
                update={
                    "status": FeatureStatus(ov["status"]) if ov.get("status") else feature.status,
                    "config": ov.get("config") if ov.get("config") is not None else feature.config,
                    "rollout_percentage": (
                        int(ov["rollout_percentage"])
                        if ov.get("rollout_percentage") is not None
                        else feature.rollout_percentage
                    ),
                }
            )
        else:
            merged[fid] = feature.model_copy()
    return merged


async def _log(
    db_session: AsyncSession,
    caller_id: str,
    action: str,
    resource_type: str,
    resource_id: str,
    changes: Optional[Dict[str, Any]] = None,
    actor_email: Optional[str] = None,
) -> None:
    await crud.create_audit_entry(
        db_session,
        caller_id,
        AuditLogEntry(
            user_id=caller_id,
            user_email=actor_email or f"{caller_id}@vimbai.com",
            action=action,
            resource_type=resource_type,
            resource_id=resource_id,
            changes=changes,
        ),
    )


# ============================================================================
# Health
# ============================================================================


@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "admin",
        "version": "1.2.0",
        "features_registered": len(FEATURES),
    }


# ============================================================================
# Feature Management
# ============================================================================


@app.get("/features")
async def list_features(
    category: Optional[FeatureCategory] = None,
    status: Optional[FeatureStatus] = None,
    enabled_only: bool = False,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all features with optional filtering"""
    result = list((await _merged_features(db_session, caller_id)).values())

    if category:
        result = [f for f in result if f.category == category]
    if status:
        result = [f for f in result if f.status == status]
    if enabled_only:
        result = [f for f in result if f.status == FeatureStatus.ENABLED]

    return result


@app.get("/features/{feature_id}")
async def get_feature(
    feature_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")
    return (await _merged_features(db_session, caller_id))[feature_id]


@app.put("/features/{feature_id}")
async def update_feature(
    feature_id: str,
    update: FeatureUpdate,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a feature (caller-scoped override)"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    await crud.upsert_feature_override(db_session, caller_id, feature_id, update)
    await _log(
        db_session,
        caller_id,
        "feature_updated",
        "feature",
        feature_id,
        {"status": update.status, "config": update.config},
    )

    return (await _merged_features(db_session, caller_id))[feature_id]


@app.post("/features/{feature_id}/enable")
async def enable_feature(
    feature_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Enable a feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    await crud.set_feature_status(db_session, caller_id, feature_id, FeatureStatus.ENABLED.value)
    return {"status": "enabled", "feature_id": feature_id}


@app.post("/features/{feature_id}/disable")
async def disable_feature(
    feature_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Disable a feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    await crud.set_feature_status(db_session, caller_id, feature_id, FeatureStatus.DISABLED.value)
    return {"status": "disabled", "feature_id": feature_id}


@app.get("/features/categories")
async def list_feature_categories():
    """List all feature categories"""
    return [{"name": cat.name, "value": cat.value} for cat in FeatureCategory]


# --- Organization Feature Configuration ---


@app.get("/organizations/{organization_id}/features")
async def get_org_features(
    organization_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get all feature configurations for an organization (caller's own org configs only)"""
    merged = await _merged_features(db_session, caller_id)
    org_configs = {
        c.feature_id: c
        for c in await crud.list_org_configs(db_session, caller_id)
        if c.organization_id == organization_id
    }

    # Merge with default features
    result = []
    for feature_id, feature in merged.items():
        if feature_id in org_configs:
            org_config = org_configs[feature_id]
            result.append(
                {
                    **feature.model_dump(),
                    "org_enabled": org_config.enabled,
                    "org_rollout_percentage": org_config.rollout_percentage,
                    "org_custom_config": org_config.custom_config,
                }
            )
        else:
            result.append(
                {
                    **feature.model_dump(),
                    "org_enabled": feature.enabled_by_default,
                    "org_rollout_percentage": feature.rollout_percentage,
                    "org_custom_config": None,
                }
            )

    return result


async def _update_org_feature(
    db_session: AsyncSession,
    caller_id: str,
    organization_id: str,
    feature_id: str,
    enabled: bool,
    custom_config: Optional[Dict[str, Any]],
    rollout_percentage: int,
    notes: Optional[str],
    updated_by: str,
) -> OrgFeatureConfig:
    """Update organization-specific feature configuration (caller-owned)."""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    existing = next(
        (
            c
            for c in await crud.list_org_configs(db_session, caller_id)
            if c.organization_id == organization_id and c.feature_id == feature_id
        ),
        None,
    )
    now = _utcnow()

    if existing:
        config = existing.model_copy(
            update={
                "enabled": enabled,
                "custom_config": custom_config,
                "rollout_percentage": rollout_percentage,
                "notes": notes,
                "enabled_at": existing.enabled_at if (enabled and existing.enabled_at) else (now if enabled else None),
                "disabled_at": None if enabled else now,
                "enabled_by": updated_by,
            }
        )
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

    saved = await crud.upsert_org_config(db_session, caller_id, config)

    await _log(
        db_session,
        caller_id,
        "org_feature_updated",
        "org_feature",
        f"{organization_id}:{feature_id}",
        {"enabled": enabled, "rollout_percentage": rollout_percentage, "organization_id": organization_id},
        actor_email=f"{updated_by}@vimbai.com",
    )

    return saved


@app.put("/organizations/{organization_id}/features/{feature_id}")
async def update_org_feature(
    organization_id: str,
    feature_id: str,
    enabled: bool,
    custom_config: Optional[Dict[str, Any]] = None,
    rollout_percentage: int = 100,
    notes: Optional[str] = None,
    updated_by: str = "admin",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update organization-specific feature configuration"""
    return await _update_org_feature(
        db_session,
        caller_id,
        organization_id,
        feature_id,
        enabled,
        custom_config,
        rollout_percentage,
        notes,
        updated_by,
    )


@app.post("/organizations/{organization_id}/features/{feature_id}/enable")
async def enable_org_feature(
    organization_id: str,
    feature_id: str,
    updated_by: str = "admin",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Enable a feature for a specific organization"""
    return await _update_org_feature(
        db_session, caller_id, organization_id, feature_id, True, None, 100, None, updated_by
    )


@app.post("/organizations/{organization_id}/features/{feature_id}/disable")
async def disable_org_feature(
    organization_id: str,
    feature_id: str,
    updated_by: str = "admin",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Disable a feature for a specific organization"""
    return await _update_org_feature(
        db_session, caller_id, organization_id, feature_id, False, None, 0, None, updated_by
    )


# --- Feature Dependencies ---


@app.get("/features/{feature_id}/dependencies")
async def get_feature_dependencies(
    feature_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get dependencies for a feature"""
    if feature_id not in FEATURES:
        raise HTTPException(status_code=404, detail="Feature not found")

    dependency = FEATURE_DEPENDENCIES.get(feature_id)
    if not dependency:
        return {"feature_id": feature_id, "dependencies": [], "satisfied": True}

    merged = await _merged_features(db_session, caller_id)

    # Check if dependencies are satisfied
    satisfied = True
    missing_deps = []
    for dep_id in dependency.depends_on:
        if dep_id in merged:
            dep_feature = merged[dep_id]
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
    status: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all feature rollout schedules"""
    result = await crud.list_schedules(db_session, caller_id)

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
    created_by: str = "admin",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
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
    saved = await crud.create_schedule(db_session, caller_id, schedule)

    await _log(
        db_session,
        caller_id,
        "rollout_scheduled",
        "rollout_schedule",
        feature_id,
        {"scheduled_date": scheduled_date, "target_percentage": target_percentage},
        actor_email=f"{created_by}@vimbai.com",
    )

    return saved


@app.delete("/rollout-schedules/{schedule_id}")
async def cancel_rollout_schedule(
    schedule_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Cancel a scheduled rollout"""
    matched = await crud.cancel_schedule(db_session, caller_id, schedule_id)
    if not matched:
        raise HTTPException(status_code=404, detail="Schedule not found")
    return {"status": "cancelled", "schedule_id": matched}


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
    business_justification: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
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
    saved = await crud.create_request(db_session, caller_id, request)

    await _log(
        db_session,
        caller_id,
        "feature_request_submitted",
        "feature_request",
        saved.id,
        {"feature_name": feature_name, "priority": priority},
        actor_email=user_email,
    )

    return saved


@app.get("/feature-requests")
async def list_feature_requests(
    status: Optional[FeatureRequestStatus] = None,
    organization_id: Optional[str] = None,
    priority: Optional[str] = None,
    limit: int = 50,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all feature requests with filters"""
    result = await crud.list_requests(db_session, caller_id)

    if status:
        result = [r for r in result if r.status == status]
    if organization_id:
        result = [r for r in result if r.organization_id == organization_id]
    if priority:
        result = [r for r in result if r.priority == priority]

    result.sort(key=lambda x: x.created_at, reverse=True)
    return result[:limit]


@app.get("/feature-requests/{request_id}")
async def get_feature_request(
    request_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific feature request"""
    request = await crud.get_request(db_session, caller_id, request_id)
    if not request:
        raise HTTPException(status_code=404, detail="Feature request not found")
    return request


@app.put("/feature-requests/{request_id}/review")
async def review_feature_request(
    request_id: str,
    status: FeatureRequestStatus,
    reviewed_by: str,
    review_notes: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Review and update a feature request status"""
    request = await crud.get_request(db_session, caller_id, request_id)
    if not request:
        raise HTTPException(status_code=404, detail="Feature request not found")

    request.status = status
    request.reviewed_by = reviewed_by
    request.reviewed_at = _utcnow()
    request.review_notes = review_notes
    request.updated_at = _utcnow()
    await crud.update_request(db_session, caller_id, request)

    await _log(
        db_session,
        caller_id,
        "feature_request_reviewed",
        "feature_request",
        request_id,
        {"status": status.value, "review_notes": review_notes},
        actor_email=f"{reviewed_by}@vimbai.com",
    )

    return request


@app.delete("/feature-requests/{request_id}")
async def delete_feature_request(
    request_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a feature request"""
    request = await crud.get_request(db_session, caller_id, request_id)
    if not request:
        raise HTTPException(status_code=404, detail="Feature request not found")

    await crud.delete_request(db_session, caller_id, request_id)
    return {"status": "deleted", "request_id": request_id}


# --- System Configuration ---


async def _merged_config(db_session: AsyncSession, caller_id: str) -> Dict[str, SystemConfig]:
    """Config defaults with the caller's Book-visible overrides applied."""
    overrides = await crud.get_config_overrides(db_session, caller_id)
    merged: Dict[str, SystemConfig] = {}
    for key, default in SYSTEM_CONFIG.items():
        ov = overrides.get(key)
        if ov:
            merged[key] = default.model_copy(
                update={"value": ov.value, "updated_at": ov.updated_at, "updated_by": ov.updated_by}
            )
        else:
            merged[key] = default.model_copy()
    return merged


@app.get("/config")
async def list_config(
    category: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List system configuration"""
    result = list((await _merged_config(db_session, caller_id)).values())

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
async def get_config(
    key: str,
    include_sensitive: bool = False,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific configuration value"""
    merged = await _merged_config(db_session, caller_id)
    if key not in merged:
        raise HTTPException(status_code=404, detail="Configuration not found")

    config = merged[key]

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
async def update_config(
    key: str,
    value: Any,
    updated_by: str = "admin",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a configuration value (caller-scoped override)"""
    if key not in SYSTEM_CONFIG:
        raise HTTPException(status_code=404, detail="Configuration not found")

    old_value = SYSTEM_CONFIG[key].value
    await crud.set_config_override(db_session, caller_id, key, value, updated_by)
    await _log(
        db_session,
        caller_id,
        "config_updated",
        "config",
        key,
        {"old_value": old_value, "new_value": value},
        actor_email=f"{updated_by}@vimbai.com",
    )

    return (await _merged_config(db_session, caller_id))[key]


# --- Audit Logs ---


@app.get("/audit-logs")
async def list_audit_logs(
    user_id: Optional[str] = None,
    action: Optional[str] = None,
    resource_type: Optional[str] = None,
    limit: int = 100,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List audit log entries (caller's own Book-visible trail)"""
    result = await crud.list_audit_entries(db_session, caller_id)

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
    changes: Optional[Dict[str, Any]] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
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
    return await crud.create_audit_entry(db_session, caller_id, entry)


# --- Service Health ---


@app.get("/services/health")
async def get_services_health():
    """Get health status of all microservices"""
    return SERVICES_HEALTH


# --- Dashboard Stats ---


@app.get("/dashboard/stats")
async def get_dashboard_stats(
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get admin dashboard statistics (over the caller's Book-visible data)"""
    merged = await _merged_features(db_session, caller_id)
    enabled_features = sum(1 for f in merged.values() if f.status == FeatureStatus.ENABLED)
    beta_features = sum(1 for f in merged.values() if f.status == FeatureStatus.BETA)
    disabled_features = sum(1 for f in merged.values() if f.status == FeatureStatus.DISABLED)

    requests = await crud.list_requests(db_session, caller_id)
    pending_requests = sum(1 for r in requests if r.status == FeatureRequestStatus.PENDING)
    approved_requests = sum(1 for r in requests if r.status == FeatureRequestStatus.APPROVED)

    schedules = await crud.list_schedules(db_session, caller_id)
    scheduled_rollouts = sum(1 for s in schedules if s.status == "scheduled")
    org_configs = await crud.list_org_configs(db_session, caller_id)
    active_org_configs = len({f"{c.organization_id}:{c.feature_id}" for c in org_configs})

    audit_entries = await crud.list_audit_entries(db_session, caller_id)

    return {
        "total_features": len(merged),
        "enabled_features": enabled_features,
        "beta_features": beta_features,
        "disabled_features": disabled_features,
        "total_config_entries": len(SYSTEM_CONFIG),
        "audit_logs_count": len(audit_entries),
        "feature_requests": {
            "pending": pending_requests,
            "approved": approved_requests,
            "total": len(requests),
        },
        "rollout_schedules": {
            "scheduled": scheduled_rollouts,
            "total": len(schedules),
        },
        "organization_configs": active_org_configs,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=int(_os.getenv("PORT", "8099")))
