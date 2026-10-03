"""
Admin Service CRUD Operations

All caller-owned (X-User-Id) and Book-gated (X-Book-ID) stores persist in
Neo4j. The code-defined catalogs (feature registry, dependencies, config
defaults, service health) live in catalog.py; only caller-specific state
(overrides, org configs, schedules, requests, audit entries) is persisted
here.
"""

import json
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from admin_service.dependencies import book_id_var
from admin_service.models import (
    AuditLogEntry,
    FeatureRequest,
    FeatureRequestStatus,
    FeatureRolloutSchedule,
    FeatureUpdate,
    OrgFeatureConfig,
    SystemConfig,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _as_dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _iso(dt: Optional[datetime]) -> Optional[str]:
    if dt is None:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.isoformat()


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


def _json_safe(value: Any) -> str:
    """JSON-dump with datetime fallback (audit change payloads may embed datetimes)."""

    def _default(o):
        if isinstance(o, datetime):
            return o.isoformat()
        raise TypeError(f"Object of type {type(o).__name__} is not JSON serializable")

    return json.dumps(value, default=_default)


# --- feature overrides (caller-scoped status/config/rollout) ---


def _override_from_node(n: Dict) -> Dict[str, Any]:
    return {
        "feature_id": n.get("feature_id", ""),
        "status": n.get("status"),
        "config": json.loads(n["config"]) if n.get("config") else None,
        "rollout_percentage": int(n["rollout_percentage"]) if n.get("rollout_percentage") is not None else None,
    }


async def _write_feature_override(
    session: AsyncSession,
    user_id: str,
    feature_id: str,
    status: Optional[str],
    config: Optional[str],
    rollout: Optional[float],
) -> None:
    """Upsert the caller's override for a feature (explicit match-then-create)."""
    existing = await get_feature_overrides(session, user_id)
    current = existing.get(feature_id) or {"status": None, "config": None, "rollout_percentage": None}
    status = status if status is not None else current["status"]
    config = config if config is not None else current["config"]
    if isinstance(config, (dict, list)):
        config = json.dumps(config)
    rollout = rollout if rollout is not None else current["rollout_percentage"]

    set_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_FEATURE_OVERRIDE]->(x:AdminFeatureOverride)
    WHERE x.feature_id = $feature_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.status = $status, x.config = $config, x.rollout_percentage = toFloat($rollout_percentage)
    """
    result = await _run(
        session,
        set_query,
        feature_id=feature_id,
        user_id=user_id,
        status=status,
        config=config,
        rollout_percentage=rollout,
    )
    records = [rec async for rec in result]
    if records:
        return

    create_query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AdminFeatureOverride {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        feature_id: $feature_id,
        status: $status,
        config: $config,
        rollout_percentage: toFloat($rollout_percentage)
    })
    CREATE (u)-[:OWNS_FEATURE_OVERRIDE]->(x)
    """
    await _run(
        session,
        create_query,
        id=str(__import__("uuid").uuid4()),
        feature_id=feature_id,
        user_id=user_id,
        status=status,
        config=config,
        rollout_percentage=rollout,
    )


async def upsert_feature_override(session: AsyncSession, user_id: str, feature_id: str, update: FeatureUpdate) -> None:
    await _write_feature_override(
        session,
        user_id,
        feature_id,
        update.status.value if update.status else None,
        json.dumps(update.config) if update.config is not None else None,
        float(update.rollout_percentage) if update.rollout_percentage is not None else None,
    )


async def get_feature_overrides(session: AsyncSession, user_id: str) -> Dict[str, Dict[str, Any]]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FEATURE_OVERRIDE]->(x:AdminFeatureOverride)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return {rec["x"]["feature_id"]: _override_from_node(dict(rec["x"])) async for rec in result}


async def set_feature_status(session: AsyncSession, user_id: str, feature_id: str, status: str) -> None:
    existing = await get_feature_overrides(session, user_id)
    current = existing.get(feature_id) or {"status": None, "config": None, "rollout_percentage": None}
    await _write_feature_override(
        session, user_id, feature_id, status, current["config"], current["rollout_percentage"]
    )


# --- org feature configs ---


def _org_config_from_node(n: Dict) -> OrgFeatureConfig:
    return OrgFeatureConfig(
        organization_id=n.get("org_id", ""),
        feature_id=n.get("feature_id", ""),
        enabled=bool(n.get("enabled", False)),
        custom_config=json.loads(n["custom_config"]) if n.get("custom_config") else None,
        rollout_percentage=int(n.get("rollout_percentage", 100)),
        enabled_at=_as_dt(n.get("enabled_at")),
        disabled_at=_as_dt(n.get("disabled_at")),
        enabled_by=n.get("enabled_by"),
        notes=n.get("notes"),
    )


async def list_org_configs(session: AsyncSession, user_id: str) -> List[OrgFeatureConfig]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ORG_FEATURE_CONFIG]->(x:AdminOrgFeatureConfig)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_org_config_from_node(dict(rec["x"])) async for rec in result]


async def upsert_org_config(session: AsyncSession, user_id: str, config: OrgFeatureConfig) -> OrgFeatureConfig:
    import uuid as _uuid

    params = {
        "id": str(_uuid.uuid4()),
        "org_id": config.organization_id,
        "feature_id": config.feature_id,
        "enabled": config.enabled,
        "custom_config": json.dumps(config.custom_config) if config.custom_config is not None else None,
        "rollout_percentage": float(config.rollout_percentage),
        "notes": config.notes,
        "enabled_at": _iso(config.enabled_at),
        "disabled_at": _iso(config.disabled_at),
        "enabled_by": config.enabled_by,
    }
    set_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ORG_FEATURE_CONFIG]->(x:AdminOrgFeatureConfig)
    WHERE x.org_id = $org_id AND x.feature_id = $feature_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.enabled = $enabled,
        x.custom_config = $custom_config,
        x.rollout_percentage = toFloat($rollout_percentage),
        x.notes = $notes,
        x.enabled_at = datetime($enabled_at),
        x.disabled_at = datetime($disabled_at),
        x.enabled_by = $enabled_by
    RETURN x
    """
    set_result = await _run(session, set_query, user_id=user_id, **params)
    records = [rec async for rec in set_result]
    if records:
        return _org_config_from_node(dict(records[0]["x"]))

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AdminOrgFeatureConfig {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        org_id: $org_id,
        feature_id: $feature_id,
        enabled: $enabled,
        custom_config: $custom_config,
        rollout_percentage: toFloat($rollout_percentage),
        notes: $notes,
        enabled_at: datetime($enabled_at),
        disabled_at: datetime($disabled_at),
        enabled_by: $enabled_by
    })
    CREATE (u)-[:OWNS_ORG_FEATURE_CONFIG]->(x)
    RETURN x
    """
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _org_config_from_node(dict(records[0]["x"]))


# --- rollout schedules ---


def _schedule_from_node(n: Dict) -> FeatureRolloutSchedule:
    return FeatureRolloutSchedule(
        feature_id=n.get("feature_id", ""),
        organization_id=n.get("org_id"),
        scheduled_date=_as_dt(n.get("scheduled_date")) or _utcnow(),
        target_percentage=int(n.get("target_percentage", 100)),
        status=n.get("status", "scheduled"),
        created_by=n.get("created_by", ""),
        created_at=_as_dt(n.get("created_at")) or _utcnow(),
    )


async def create_schedule(session: AsyncSession, user_id: str, s: FeatureRolloutSchedule) -> FeatureRolloutSchedule:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AdminRolloutSchedule {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        feature_id: $feature_id,
        org_id: $org_id,
        scheduled_date: datetime($scheduled_date),
        target_percentage: toFloat($target_percentage),
        status: $status,
        created_by: $created_by,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_SCHEDULE]->(x)
    RETURN x
    """
    import uuid as _uuid

    params = {
        "id": str(_uuid.uuid4()),
        "feature_id": s.feature_id,
        "org_id": s.organization_id,
        "scheduled_date": _iso(s.scheduled_date),
        "target_percentage": float(s.target_percentage),
        "status": s.status,
        "created_by": s.created_by,
        "created_at": _iso(s.created_at or _utcnow()),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _schedule_from_node(dict(records[0]["x"]))


async def list_schedules(session: AsyncSession, user_id: str) -> List[FeatureRolloutSchedule]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCHEDULE]->(x:AdminRolloutSchedule)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_schedule_from_node(dict(rec["x"])) async for rec in result]


async def cancel_schedule(session: AsyncSession, user_id: str, schedule_id: str) -> Optional[str]:
    """Cancel by schedule id OR feature id (original semantics); returns matched id or None."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCHEDULE]->(x:AdminRolloutSchedule)
    WHERE (x.id = $schedule_id OR x.feature_id = $schedule_id)
    AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.status = 'cancelled'
    RETURN x
    """
    result = await _run(session, query, schedule_id=schedule_id, user_id=user_id)
    record = await result.single()
    return dict(record["x"]).get("id") if record else None


# --- feature requests ---


def _request_from_node(n: Dict) -> FeatureRequest:
    return FeatureRequest(
        id=n["id"],
        user_id=n.get("req_user_id", ""),
        user_email=n.get("user_email", ""),
        organization_id=n.get("org_id"),
        feature_name=n.get("feature_name", ""),
        feature_description=n.get("feature_description"),
        category=n.get("category"),
        priority=n.get("priority", "normal"),
        business_justification=n.get("business_justification"),
        status=n.get("status", "pending"),
        reviewed_by=n.get("reviewed_by"),
        reviewed_at=_as_dt(n.get("reviewed_at")),
        review_notes=n.get("review_notes"),
        created_at=_as_dt(n.get("created_at")) or _utcnow(),
        updated_at=_as_dt(n.get("updated_at")) or _utcnow(),
    )


async def create_request(session: AsyncSession, user_id: str, r: FeatureRequest) -> FeatureRequest:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AdminFeatureRequest {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        req_user_id: $req_user_id,
        user_email: $user_email,
        org_id: $org_id,
        feature_name: $feature_name,
        feature_description: $feature_description,
        category: $category,
        priority: $priority,
        business_justification: $business_justification,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_REQUEST]->(x)
    RETURN x
    """
    params = {
        "id": r.id,
        "req_user_id": r.user_id,
        "user_email": r.user_email,
        "org_id": r.organization_id,
        "feature_name": r.feature_name,
        "feature_description": r.feature_description,
        "category": r.category.value if r.category else None,
        "priority": r.priority,
        "business_justification": r.business_justification,
        "status": r.status.value if r.status else "pending",
        "created_at": _iso(r.created_at or _utcnow()),
        "updated_at": _iso(r.updated_at or _utcnow()),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _request_from_node(dict(records[0]["x"]))


async def list_requests(session: AsyncSession, user_id: str) -> List[FeatureRequest]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REQUEST]->(x:AdminFeatureRequest)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_request_from_node(dict(rec["x"])) async for rec in result]


async def get_request(session: AsyncSession, user_id: str, request_id: str) -> Optional[FeatureRequest]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_REQUEST]->(x:AdminFeatureRequest)
    WHERE x.id = $request_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, request_id=request_id, user_id=user_id)
    record = await result.single()
    return _request_from_node(dict(record["x"])) if record else None


async def update_request(session: AsyncSession, user_id: str, r: FeatureRequest) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_REQUEST]->(x:AdminFeatureRequest)
    WHERE x.id = $request_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.status = $status,
        x.reviewed_by = $reviewed_by,
        x.reviewed_at = datetime($reviewed_at),
        x.review_notes = $review_notes,
        x.updated_at = datetime($updated_at)
    """
    params = {
        "request_id": r.id,
        "status": r.status.value if r.status else "pending",
        "reviewed_by": r.reviewed_by,
        "reviewed_at": _iso(r.reviewed_at),
        "review_notes": r.review_notes,
        "updated_at": _iso(_utcnow()),
    }
    await _run(session, query, params, user_id=user_id)


async def delete_request(session: AsyncSession, user_id: str, request_id: str) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_REQUEST]->(x:AdminFeatureRequest)
    WHERE x.id = $request_id AND ($book_id IS NULL OR x.book_id = $book_id)
    DETACH DELETE x
    """
    await _run(session, query, request_id=request_id, user_id=user_id)


# --- system config overrides ---


async def set_config_override(session: AsyncSession, user_id: str, key: str, value: Any, updated_by: str) -> None:
    set_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CONFIG_OVERRIDE]->(x:AdminConfigOverride)
    WHERE x.config_key = $key AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.value = $value, x.updated_by = $updated_by, x.updated_at = datetime($updated_at)
    """
    result = await _run(
        session,
        set_query,
        key=key,
        value=json.dumps(value),
        updated_by=updated_by,
        updated_at=_iso(_utcnow()),
        user_id=user_id,
    )
    records = [rec async for rec in result]
    if records:
        return

    create_query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AdminConfigOverride {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        config_key: $key,
        value: $value,
        updated_by: $updated_by,
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_CONFIG_OVERRIDE]->(x)
    """
    import uuid as _uuid

    await _run(
        session,
        create_query,
        id=str(_uuid.uuid4()),
        key=key,
        value=json.dumps(value),
        updated_by=updated_by,
        updated_at=_iso(_utcnow()),
        user_id=user_id,
    )


async def get_config_overrides(session: AsyncSession, user_id: str) -> Dict[str, SystemConfig]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONFIG_OVERRIDE]->(x:AdminConfigOverride)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    out: Dict[str, SystemConfig] = {}
    async for rec in result:
        n = dict(rec["x"])
        out[n["config_key"]] = SystemConfig(
            key=n["config_key"],
            value=json.loads(n["value"]) if n.get("value") is not None else None,
            description=None,
            category="",
            updated_at=_as_dt(n.get("updated_at")) or _utcnow(),
            updated_by=n.get("updated_by"),
        )
    return out


# --- audit logs ---


def _audit_from_node(n: Dict) -> AuditLogEntry:
    return AuditLogEntry(
        id=n["id"],
        timestamp=_as_dt(n.get("timestamp")) or _utcnow(),
        user_id=n.get("actor_user_id", ""),
        user_email=n.get("user_email", ""),
        action=n.get("action", ""),
        resource_type=n.get("resource_type", ""),
        resource_id=n.get("resource_id", ""),
        changes=json.loads(n["changes"]) if n.get("changes") else None,
        ip_address=n.get("ip_address"),
        user_agent=n.get("user_agent"),
    )


async def create_audit_entry(session: AsyncSession, user_id: str, e: AuditLogEntry) -> AuditLogEntry:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AdminAuditLog {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        actor_user_id: $actor_user_id,
        user_email: $user_email,
        action: $action,
        resource_type: $resource_type,
        resource_id: $resource_id,
        changes: $changes,
        ip_address: $ip_address,
        user_agent: $user_agent,
        timestamp: datetime($timestamp)
    })
    CREATE (u)-[:OWNS_AUDIT_LOG]->(x)
    RETURN x
    """
    params = {
        "id": e.id,
        "actor_user_id": e.user_id,
        "user_email": e.user_email,
        "action": e.action,
        "resource_type": e.resource_type,
        "resource_id": e.resource_id,
        "changes": _json_safe(e.changes) if e.changes is not None else None,
        "ip_address": e.ip_address,
        "user_agent": e.user_agent,
        "timestamp": _iso(e.timestamp or _utcnow()),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _audit_from_node(dict(records[0]["x"]))


async def list_audit_entries(session: AsyncSession, user_id: str) -> List[AuditLogEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_AUDIT_LOG]->(x:AdminAuditLog)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_audit_from_node(dict(rec["x"])) async for rec in result]
