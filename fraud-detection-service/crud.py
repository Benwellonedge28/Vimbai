"""
Fraud Detection Service CRUD Operations

Fraud alerts and detection rules move from in-memory dicts
(_alerts_store / _rules_store) to Neo4j: :FraudAlert nodes via
:OWNS_ALERT edges and :FraudRule nodes via :OWNS_RULE edges, book_id
stamped, Book-gated. Rule evaluation itself stays a pure computation
(engine.py); default rules are seeded and persisted per
user+company+Book on first access, preserving the original
lazy-seed semantics.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from fraud_detection_service.dependencies import book_id_var
from fraud_detection_service.exceptions import NotFoundError
from fraud_detection_service.models import (
    DEFAULT_RULES,
    FraudAlert,
    FraudRule,
    FraudSeverity,
    FraudStatus,
    RiskAssessment,
    RiskLevel,
)
from neo4j import AsyncSession


def _level(score: float) -> RiskLevel:
    """Same thresholds as engine.calculate_risk_level."""
    from fraud_detection_service.engine import calculate_risk_level

    return calculate_risk_level(score)


BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


# --------------------------------------------------------------------------
# Rules
# --------------------------------------------------------------------------


def _rule_from_node(n: Dict[str, Any]) -> FraudRule:
    return FraudRule(
        id=n["id"],
        name=n["name"],
        description=n["description"],
        rule_type=n["rule_type"],
        parameters=json.loads(n.get("parameters", "{}") or "{}"),
        severity=FraudSeverity(n.get("severity", "medium")),
        enabled=bool(n.get("enabled", 1)),
    )


async def _create_rule_node(session: AsyncSession, user_id: str, company_id: str, rule: FraudRule) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:FraudRule {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        name: $name,
        description: $description,
        rule_type: $rule_type,
        parameters: $parameters,
        severity: $severity,
        enabled: $enabled,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_RULE]->(x)
    """
    await _run(
        session,
        query,
        id=rule.id,
        user_id=user_id,
        company_id=company_id,
        name=rule.name,
        description=rule.description,
        rule_type=rule.rule_type,
        parameters=json.dumps(rule.parameters),
        severity=rule.severity.value,
        enabled=rule.enabled,
        created_at=_now().isoformat(),
    )


async def get_or_seed_rules(session: AsyncSession, user_id: str, company_id: str) -> List[FraudRule]:
    """Return the caller's rules for the company, seeding defaults on first access."""
    rules = await list_rules(session, user_id, company_id)
    if rules:
        return rules
    seeded = []
    for template in DEFAULT_RULES:
        rule = FraudRule(
            id=str(uuid.uuid4()),
            name=template.name,
            description=template.description,
            rule_type=template.rule_type,
            parameters=template.parameters,
            severity=template.severity,
            enabled=template.enabled,
        )
        await _create_rule_node(session, user_id, company_id, rule)
        seeded.append(rule)
    return seeded


async def list_rules(session: AsyncSession, user_id: str, company_id: str) -> List[FraudRule]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RULE]->(x:FraudRule {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_rule_from_node(dict(r["x"])) async for r in result]


async def add_rule(session: AsyncSession, user_id: str, company_id: str, rule: FraudRule) -> FraudRule:
    await _create_rule_node(session, user_id, company_id, rule)
    return rule


async def toggle_rule(session: AsyncSession, user_id: str, rule_id: str, enabled: bool) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RULE]->(x:FraudRule {{id: $rule_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, rule_id=rule_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Rule not found")
    query = "MATCH (x:FraudRule {id: $id}) SET x.enabled = $enabled"
    await _run(session, query, id=rule_id, enabled=enabled)
    return {"rule_id": rule_id, "enabled": enabled}


# --------------------------------------------------------------------------
# Alerts
# --------------------------------------------------------------------------


def _coerce_dt(value):
    """Coerce a datetime property to a real datetime.

    Real Neo4j drivers return datetime subclasses; the fake harness
    returns a Temporal wrapper with iso_format().
    """
    if value is None or isinstance(value, datetime):
        return value
    iso = getattr(value, "iso_format", None)
    if callable(iso):
        s = iso()
        if not s:
            return None
        return datetime.fromisoformat(s.replace("Z", "+00:00"))
    return value


def _alert_from_node(n: Dict[str, Any]) -> FraudAlert:
    return FraudAlert(
        id=n["id"],
        transaction_id=n["transaction_id"],
        company_id=n["company_id"],
        rule_id=n["rule_id"],
        rule_name=n["rule_name"],
        severity=FraudSeverity(n.get("severity", "medium")),
        risk_score=float(n.get("risk_score", 0)),
        description=n["description"],
        detected_at=_coerce_dt(n.get("detected_at")),
        status=FraudStatus(n.get("status", "pending")),
        details=json.loads(n.get("details", "{}") or "{}"),
    )


async def store_alerts(session: AsyncSession, user_id: str, alerts: List[FraudAlert]) -> None:
    for alert in alerts:
        query = f"""
        MATCH (u:User {{id: $user_id}})
        CREATE (x:FraudAlert {{
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            transaction_id: $transaction_id,
            company_id: $company_id,
            rule_id: $rule_id,
            rule_name: $rule_name,
            severity: $severity,
            risk_score: toFloat($risk_score),
            description: $description,
            detected_at: datetime($detected_at),
            status: $status,
            details: $details
        }})
        CREATE (u)-[:OWNS_ALERT]->(x)
        """
        await _run(
            session,
            query,
            id=alert.id,
            user_id=user_id,
            transaction_id=alert.transaction_id,
            company_id=alert.company_id,
            rule_id=alert.rule_id,
            rule_name=alert.rule_name,
            severity=alert.severity.value,
            risk_score=alert.risk_score,
            description=alert.description,
            detected_at=_now().isoformat(),
            status=alert.status.value,
            details=json.dumps(alert.details),
        )


async def list_alerts(
    session: AsyncSession, user_id: str, company_id: str, status_filter: Optional[str] = None
) -> List[FraudAlert]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ALERT]->(x:FraudAlert {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.detected_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    alerts = [_alert_from_node(dict(r["x"])) async for r in result]
    if status_filter:
        alerts = [a for a in alerts if a.status.value == status_filter]
    return alerts


async def update_alert_status(
    session: AsyncSession, user_id: str, alert_id: str, new_status: FraudStatus
) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ALERT]->(x:FraudAlert {{id: $alert_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, alert_id=alert_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Alert not found")
    query = "MATCH (x:FraudAlert {id: $id}) SET x.status = $status"
    await _run(session, query, id=alert_id, status=new_status.value)
    return {"alert_id": alert_id, "status": new_status.value}


async def get_risk_assessment(session: AsyncSession, user_id: str, company_id: str) -> RiskAssessment:
    alerts = await list_alerts(session, user_id, company_id)
    if not alerts:
        return RiskAssessment(
            company_id=company_id,
            overall_risk_level=RiskLevel.MINIMAL,
            risk_score=0,
            total_transactions=0,
            flagged_transactions=0,
            alerts=[],
        )
    max_score = max(a.risk_score for a in alerts)
    return RiskAssessment(
        company_id=company_id,
        overall_risk_level=_level(max_score),
        risk_score=max_score,
        total_transactions=len(set(a.transaction_id for a in alerts)),
        flagged_transactions=len(set(a.transaction_id for a in alerts)),
        alerts=alerts,
    )
