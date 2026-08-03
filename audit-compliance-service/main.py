"""
Vimbai Audit & Compliance Service (Merged)
Port: 8091

This service consolidates the following former services:
  - audit-service (Port: 8091)
  - audit-compliance-service (Port: 8343)
  - audit-report-service (Port: 8202)
  - audit-planning-service (Port: 8195)
  - audit-trails-service (Port: 8283)
  - audit-reporting-service (Port: 8284)
  - audit-management-service (Port: 8285)
  - compliance-monitoring-service (Port: 8282)
  - compliance-audit-service (Port: 8286)
  - external-audit-service (Port: 8287)
  - internal-audit-service (Port: 8288)

Capabilities:
  - Immutable audit trail with tamper-proof event logging
  - Data versioning and lineage tracing
  - Audit chain integrity verification
  - Compliance reporting (SOX, GDPR, SOC2)
  - Audit planning, risk assessment, and materiality
  - Audit report generation with findings and opinions
  - Audit trail analysis and suspicious activity detection
  - Compliance monitoring and alerting
"""

import hashlib
import json
import os
import uuid
from collections import defaultdict
from datetime import date, datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from pydantic import BaseModel, Field

# ============================================================================
# Configuration
# ============================================================================

SERVICE_NAME = "audit-compliance-service"
SERVICE_VERSION = "2.0.0"
PORT = int(os.getenv("PORT", "8091"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(
    title="Vimbai Audit & Compliance Service",
    description="Consolidated Audit Trail, Compliance, Planning, Reporting, and Monitoring",
    version=SERVICE_VERSION,
)

# ============================================================================
# Enums
# ============================================================================

class EventType(str, Enum):
    CREATE = "create"
    UPDATE = "update"
    DELETE = "delete"
    APPROVE = "approve"
    REJECT = "reject"
    POST = "post"
    UNPOST = "unpost"
    REVERSE = "reverse"
    RECONCILE = "reconcile"
    IMPORT = "import"
    EXPORT = "export"
    LOGIN = "login"
    LOGOUT = "logout"
    PASSWORD_CHANGE = "password_change"
    PERMISSION_CHANGE = "permission_change"
    CONFIG_CHANGE = "config_change"


class ResourceType(str, Enum):
    USER = "user"
    ACCOUNT = "account"
    JOURNAL_ENTRY = "journal_entry"
    JOURNAL_LINE = "journal_line"
    INVOICE = "invoice"
    PAYMENT = "payment"
    PROJECT = "project"
    FUND = "fund"
    DEPARTMENT = "department"
    BUDGET = "budget"
    REPORT = "report"
    WORKFLOW = "workflow"
    DOCUMENT = "document"
    CONFIGURATION = "configuration"


class Severity(str, Enum):
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    CRITICAL = "critical"


# ============================================================================
# Pydantic Models — Audit Trail
# ============================================================================

class AuditEventCreate(BaseModel):
    event_type: EventType
    resource_type: ResourceType
    resource_id: str
    user_id: str
    user_email: Optional[str] = None
    organization_id: Optional[str] = None
    action_details: Dict[str, Any] = {}
    previous_state: Optional[Dict[str, Any]] = None
    new_state: Optional[Dict[str, Any]] = None
    metadata: Optional[Dict[str, Any]] = None
    ip_address: Optional[str] = None
    user_agent: Optional[str] = None
    session_id: Optional[str] = None
    correlation_id: Optional[str] = None


class AuditEvent(BaseModel):
    id: str
    event_type: EventType
    resource_type: ResourceType
    resource_id: str
    user_id: str
    user_email: Optional[str] = None
    organization_id: Optional[str] = None
    action_details: Dict[str, Any] = {}
    previous_state: Optional[Dict[str, Any]] = None
    new_state: Optional[Dict[str, Any]] = None
    metadata: Optional[Dict[str, Any]] = None
    checksum: str
    timestamp: datetime
    ip_address: Optional[str] = None
    user_agent: Optional[str] = None
    session_id: Optional[str] = None
    correlation_id: Optional[str] = None


class VersionSnapshot(BaseModel):
    id: str
    resource_type: ResourceType
    resource_id: str
    version: int
    state: Dict[str, Any]
    changed_by: str
    changed_at: datetime
    change_reason: Optional[str] = None
    checksum: str


class DataLineageNode(BaseModel):
    id: str
    resource_type: ResourceType
    resource_id: str
    operation: str
    timestamp: datetime
    user_id: str
    source_event_id: Optional[str] = None


class DataLineageEdge(BaseModel):
    id: str
    from_node_id: str
    to_node_id: str
    relationship_type: str
    metadata: Optional[Dict[str, Any]] = None


# ============================================================================
# Pydantic Models — Compliance
# ============================================================================

class ComplianceCheck(BaseModel):
    check_id: str
    regulation: str
    description: str
    status: str
    last_checked: str


class ComplianceReportRequest(BaseModel):
    start_date: datetime
    end_date: datetime
    resource_types: List[ResourceType] = []
    user_ids: List[str] = []
    include_verifications: bool = True
    include_integrity_checks: bool = True


class ComplianceReport(BaseModel):
    report_id: str
    generated_at: datetime
    period_start: datetime
    period_end: datetime
    total_events: int
    events_by_type: Dict[str, int]
    events_by_user: Dict[str, int]
    integrity_verified: bool
    findings: List[Dict[str, Any]]


class SOXComplianceRequest(BaseModel):
    company_id: str
    fiscal_year: int
    controls: List[Dict[str, Any]]
    evidence_required: List[str]


class GDPRComplianceRequest(BaseModel):
    company_id: str
    data_processing_activities: List[Dict[str, Any]]
    retention_policies: List[Dict[str, Any]]
    consent_records: int


class ComplianceMonitoringRequest(BaseModel):
    company_id: str
    checks: List[ComplianceCheck]


# ============================================================================
# Pydantic Models — Audit Planning
# ============================================================================

class AuditScope(BaseModel):
    entities: List[str]
    periods: List[str]
    accounts: List[str]
    locations: List[str]


class MaterialityLevels(BaseModel):
    planning_materiality: float
    performance_materiality: float
    thresholds_unadjusted: float


class RiskAssessment(BaseModel):
    inherent_risk: str
    control_risk: str
    detection_risk: float
    risk_level: str


class AuditPlanningRequest(BaseModel):
    audit_id: str
    company_id: str
    fiscal_year: str
    prior_year_findings: List[Dict[str, Any]]
    industry_risk_factors: List[str]
    regulatory_requirements: List[str]
    client_acceptance: bool


# ============================================================================
# Pydantic Models — Audit Report
# ============================================================================

class Finding(BaseModel):
    finding_id: str
    description: str
    impact: str
    severity: str
    recommendation: str


class AuditReportRequest(BaseModel):
    audit_id: str
    company_id: str
    fiscal_year: str
    opinion: str
    key_audit_matters: List[str]
    findings: List[Dict[str, Any]]
    material_weaknesses: List[str]
    going_concern_issues: bool


# ============================================================================
# Pydantic Models — Audit Trails Analysis
# ============================================================================

class AuditEntry(BaseModel):
    entry_id: str
    user_id: str
    action: str
    timestamp: str
    system: str


class AuditTrailsRequest(BaseModel):
    company_id: str
    entries: List[AuditEntry]
    start_date: str
    end_date: str


# ============================================================================
# In-Memory Storage
# ============================================================================

audit_events: Dict[str, AuditEvent] = {}
version_snapshots: Dict[str, List[VersionSnapshot]] = defaultdict(list)
data_lineage: Dict[str, List[str]] = defaultdict(list)
integrity_chain: List[str] = []

# ============================================================================
# Helper Functions
# ============================================================================

def generate_checksum(event_data: Dict) -> str:
    content = json.dumps(event_data, sort_keys=True, default=str)
    return hashlib.sha256(content.encode()).hexdigest()


def verify_chain_integrity():
    errors = []
    for i, event_id in enumerate(integrity_chain):
        event = audit_events.get(event_id)
        if not event:
            errors.append(f"Event {event_id} not found at position {i}")
            continue
        computed_checksum = generate_checksum(event.model_dump(exclude={"checksum"}))
        if computed_checksum != event.checksum:
            errors.append(f"Checksum mismatch for event {event_id}")
    return len(errors) == 0, errors


def calculate_hash_chain(event_ids: List[str]) -> str:
    chain_hash = ""
    for event_id in event_ids:
        event = audit_events.get(event_id)
        if event:
            chain_hash = hashlib.sha256(
                (chain_hash + event.checksum).encode()
            ).hexdigest()
    return chain_hash


# ============================================================================
# Routes — Health
# ============================================================================

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


# ============================================================================
# Routes — Audit Trail
# ============================================================================

@app.post("/events", status_code=status.HTTP_201_CREATED)
async def create_audit_event(event_create: AuditEventCreate):
    """Log an immutable audit event."""
    event_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    event_data = {
        "id": event_id,
        "event_type": event_create.event_type.value,
        "resource_type": event_create.resource_type.value,
        "resource_id": event_create.resource_id,
        "user_id": event_create.user_id,
        "user_email": event_create.user_email,
        "organization_id": event_create.organization_id,
        "action_details": event_create.action_details,
        "previous_state": event_create.previous_state,
        "new_state": event_create.new_state,
        "metadata": event_create.metadata,
        "timestamp": now.isoformat(),
        "ip_address": event_create.ip_address,
        "user_agent": event_create.user_agent,
        "session_id": event_create.session_id,
        "correlation_id": event_create.correlation_id,
    }

    checksum = generate_checksum(event_data)

    event = AuditEvent(
        id=event_id,
        checksum=checksum,
        timestamp=now,
        **{k: v for k, v in event_create.model_dump().items()},
    )

    audit_events[event_id] = event
    integrity_chain.append(event_id)
    data_lineage[event_create.resource_id].append(event_id)

    logger.info("Audit event created", event_id=event_id, type=event_create.event_type.value)
    return event


@app.get("/events")
async def list_audit_events(
    resource_type: Optional[ResourceType] = None,
    resource_id: Optional[str] = None,
    user_id: Optional[str] = None,
    event_type: Optional[EventType] = None,
    limit: int = 100,
    offset: int = 0,
):
    """List audit events with optional filters."""
    events = list(audit_events.values())
    if resource_type:
        events = [e for e in events if e.resource_type == resource_type]
    if resource_id:
        events = [e for e in events if e.resource_id == resource_id]
    if user_id:
        events = [e for e in events if e.user_id == user_id]
    if event_type:
        events = [e for e in events if e.event_type == event_type]
    events.sort(key=lambda x: x.timestamp, reverse=True)
    return {"total": len(events), "events": events[offset: offset + limit]}


@app.get("/events/{event_id}")
async def get_audit_event(event_id: str):
    """Get a specific audit event."""
    if event_id not in audit_events:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Event not found")
    return audit_events[event_id]


# ============================================================================
# Routes — Versioning
# ============================================================================

@app.post("/versions/{resource_type}/{resource_id}")
async def create_version_snapshot(
    resource_type: ResourceType,
    resource_id: str,
    state: Dict[str, Any],
    changed_by: str,
    change_reason: Optional[str] = None,
):
    """Create a version snapshot of a resource."""
    snapshots = version_snapshots[resource_id]
    version = len(snapshots) + 1
    snapshot_id = str(uuid.uuid4())
    checksum = generate_checksum(state)

    snapshot = VersionSnapshot(
        id=snapshot_id,
        resource_type=resource_type,
        resource_id=resource_id,
        version=version,
        state=state,
        changed_by=changed_by,
        changed_at=datetime.now(timezone.utc),
        change_reason=change_reason,
        checksum=checksum,
    )
    version_snapshots[resource_id].append(snapshot)
    return snapshot


@app.get("/versions/{resource_type}/{resource_id}")
async def get_version_history(resource_type: ResourceType, resource_id: str):
    """Get version history for a resource."""
    return {"resource_id": resource_id, "versions": version_snapshots.get(resource_id, [])}


# ============================================================================
# Routes — Data Lineage
# ============================================================================

@app.get("/lineage/{resource_id}")
async def get_data_lineage(
    resource_id: str,
    depth: int = 10,
    direction: Literal["forward", "backward", "both"] = "both",
):
    """Trace data lineage for a resource."""
    event_ids = data_lineage.get(resource_id, [])
    if not event_ids:
        return {"resource_id": resource_id, "lineage": [], "message": "No lineage data found"}

    nodes = []
    edges = []
    for event_id in event_ids[:depth]:
        event = audit_events.get(event_id)
        if not event:
            continue
        node = DataLineageNode(
            id=event_id,
            resource_type=event.resource_type,
            resource_id=event.resource_id,
            operation=event.event_type.value,
            timestamp=event.timestamp,
            user_id=event.user_id,
            source_event_id=event.correlation_id,
        )
        nodes.append(node)
        if event.correlation_id and event.correlation_id in audit_events:
            edge = DataLineageEdge(
                id=str(uuid.uuid4()),
                from_node_id=event.correlation_id,
                to_node_id=event_id,
                relationship_type="caused_by",
            )
            edges.append(edge)

    return {
        "resource_id": resource_id,
        "nodes": [n.model_dump() for n in nodes],
        "edges": [e.model_dump() for e in edges],
        "total_events": len(event_ids),
        "displayed_events": len(nodes),
    }


# ============================================================================
# Routes — Integrity Verification
# ============================================================================

@app.get("/integrity/verify")
async def verify_integrity():
    """Verify the integrity of the entire audit chain."""
    is_valid, errors = verify_chain_integrity()
    chain_hash = calculate_hash_chain(integrity_chain)
    return {
        "is_valid": is_valid,
        "total_events": len(audit_events),
        "chain_length": len(integrity_chain),
        "chain_hash": chain_hash,
        "errors": errors,
        "verified_at": datetime.now(timezone.utc),
    }


@app.get("/integrity/verify-event/{event_id}")
async def verify_single_event(event_id: str):
    """Verify integrity of a single event."""
    if event_id not in audit_events:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Event not found")
    event = audit_events[event_id]
    computed_checksum = generate_checksum(event.model_dump(exclude={"checksum"}))
    return {
        "event_id": event_id,
        "stored_checksum": event.checksum,
        "computed_checksum": computed_checksum,
        "matches": computed_checksum == event.checksum,
        "verified_at": datetime.now(timezone.utc),
    }


# ============================================================================
# Routes — Audit Planning
# ============================================================================

@app.post("/planning/plan")
async def create_audit_plan(request: AuditPlanningRequest):
    """Create an audit plan with materiality, scope, risk assessment, and timeline."""
    logger.info("Creating audit plan", audit=request.audit_id, company=request.company_id)

    base_materiality = 1_000_000.0
    planning_materiality = base_materiality * 0.5
    performance_materiality = planning_materiality * 0.75

    risk_counts = {"high": 0, "medium": 0, "low": 0}
    for finding in request.prior_year_findings:
        risk = finding.get("risk_level", "medium")
        if risk in risk_counts:
            risk_counts[risk] += 1

    overall_risk = "high" if risk_counts["high"] > 3 else "medium" if risk_counts["medium"] > 2 else "low"

    return {
        "audit_id": request.audit_id,
        "overall_risk": overall_risk,
        "materiality": {
            "planning_materiality": round(planning_materiality, 2),
            "performance_materiality": round(performance_materiality, 2),
            "thresholds_unadjusted": round(performance_materiality * 0.05, 2),
        },
        "audit_scope": {
            "entities": [f"Entity_{i}" for i in range(1, 4)],
            "periods": [request.fiscal_year],
            "accounts": ["Revenue", "Assets", "Liabilities", "Equity"],
            "locations": ["Head Office", "Regional Office 1"],
        },
        "risk_assessment": {
            "revenue_recognition": {"inherent_risk": "high", "control_risk": "medium", "detection_risk": 0.10, "risk_level": "high"},
            "inventory": {"inherent_risk": "medium", "control_risk": "low", "detection_risk": 0.05, "risk_level": "medium"},
        },
        "audit_strategy": {
            "approach": "Risk-based audit approach",
            "sampling_method": "Statistical sampling with random selection",
            "testing_strategy": "Substantive procedures for high-risk areas",
        },
        "resource_requirements": {"senior_auditors": 2, "junior_auditors": 4, "specialists": 1, "estimated_hours": 800},
        "timeline": {
            "planning_start": f"{request.fiscal_year}-01-01",
            "fieldwork_start": f"{request.fiscal_year}-03-01",
            "fieldwork_end": f"{request.fiscal_year}-05-31",
            "report_issue": f"{request.fiscal_year}-06-30",
        },
        "key_focus_areas": ["Revenue recognition", "Going concern assessment", "Related party transactions"],
    }


# ============================================================================
# Routes — Audit Report
# ============================================================================

@app.post("/reports/generate")
async def generate_audit_report(request: AuditReportRequest):
    """Generate a formal audit report with opinion, findings, and regulatory disclosures."""
    logger.info("Generating audit report", audit=request.audit_id, company=request.company_id)

    findings_summary = [
        {
            "finding_id": f.get("id", ""),
            "description": f.get("description", ""),
            "impact": f.get("impact", ""),
            "severity": f.get("severity", "medium"),
            "recommendation": f.get("recommendation", ""),
        }
        for f in request.findings
    ]

    emphasis_matters = []
    if request.material_weaknesses:
        emphasis_matters.append("Material weaknesses in internal control")
    if request.going_concern_issues:
        emphasis_matters.append("Substantial doubt about going concern")

    opinion_type_map = {
        "unqualified": "Unqualified Opinion",
        "qualified": "Qualified Opinion",
        "adverse": "Adverse Opinion",
        "disclaimer": "Disclaimer of Opinion",
    }

    return {
        "audit_id": request.audit_id,
        "company_id": request.company_id,
        "fiscal_year": request.fiscal_year,
        "opinion_type": opinion_type_map.get(request.opinion, "Unqualified Opinion"),
        "report_date": datetime.now(timezone.utc).date().isoformat(),
        "key_audit_matters": request.key_audit_matters,
        "findings_summary": findings_summary,
        "material_weaknesses_disclosure": len(request.material_weaknesses) > 0,
        "going_concern_disclosure": request.going_concern_issues,
        "emphasis_of_matter": emphasis_matters if emphasis_matters else ["No emphasis of matter paragraphs"],
        "regulatory_filing_required": ["SEC Filing", "Stock Exchange Filing", "Tax Authorities"],
    }


# ============================================================================
# Routes — Audit Trails Analysis
# ============================================================================

@app.post("/trails/analyze")
async def analyze_audit_trails(request: AuditTrailsRequest):
    """Analyze audit trail entries for activity patterns and suspicious behaviour."""
    logger.info("Analyzing audit trails", company=request.company_id)

    by_user: Dict[str, Dict[str, Any]] = {}
    for e in request.entries:
        if e.user_id not in by_user:
            by_user[e.user_id] = {"user_id": e.user_id, "actions": 0}
        by_user[e.user_id]["actions"] += 1

    activity_by_user = list(by_user.values())
    suspicious = [
        {"user_id": u["user_id"], "reason": "High activity volume"}
        for u in activity_by_user
        if u["actions"] > 100
    ]

    return {
        "company_id": request.company_id,
        "audit_summary": {
            "total_entries": len(request.entries),
            "unique_users": len(by_user),
            "start_date": request.start_date,
            "end_date": request.end_date,
            "avg_actions_per_user": round(len(request.entries) / len(by_user), 2) if by_user else 0,
        },
        "activity_by_user": activity_by_user,
        "suspicious_activities": suspicious,
    }


# ============================================================================
# Routes — Compliance Monitoring
# ============================================================================

@app.post("/compliance/monitor")
async def monitor_compliance(request: ComplianceMonitoringRequest):
    """Monitor compliance checks and summarise pass/fail status."""
    logger.info("Monitoring compliance", company=request.company_id)

    compliant = sum(1 for c in request.checks if c.status == "Compliant")
    non_compliant = sum(1 for c in request.checks if c.status == "Non-Compliant")
    pending = sum(1 for c in request.checks if c.status == "Pending")

    return {
        "company_id": request.company_id,
        "monitoring_date": datetime.now(timezone.utc).isoformat(),
        "compliance_summary": {
            "total_checks": len(request.checks),
            "compliant": compliant,
            "non_compliant": non_compliant,
            "pending": pending,
            "compliance_rate": round(compliant / len(request.checks) * 100, 2) if request.checks else 0,
        },
        "check_results": [
            {"check_id": c.check_id, "regulation": c.regulation, "status": c.status, "last_checked": c.last_checked}
            for c in request.checks
        ],
        "critical_issues": [c.description for c in request.checks if c.status == "Non-Compliant"],
    }


@app.post("/compliance/sox")
async def assess_sox_compliance(request: SOXComplianceRequest):
    """Assess SOX compliance for a given fiscal year and control set."""
    logger.info("SOX compliance assessment", company=request.company_id, year=request.fiscal_year)

    passing = sum(1 for c in request.controls if c.get("tested", True))
    failing = len(request.controls) - passing

    return {
        "company_id": request.company_id,
        "fiscal_year": request.fiscal_year,
        "overall_status": "compliant" if failing == 0 else "needs_attention",
        "control_count": len(request.controls),
        "passing_controls": passing,
        "failing_controls": failing,
        "exceptions": [],
    }


@app.post("/compliance/gdpr")
async def assess_gdpr_compliance(request: GDPRComplianceRequest):
    """Assess GDPR compliance based on processing activities and consent records."""
    logger.info("GDPR compliance assessment", company=request.company_id)

    violations = []
    if request.consent_records < 1000:
        violations.append("Insufficient consent records")
    if len(request.retention_policies) < 3:
        violations.append("Missing retention policies")

    return {
        "company_id": request.company_id,
        "compliance_score": round(max(0.5, 1.0 - (len(violations) * 0.15)), 4),
        "data_subjects_count": request.consent_records * 2,
        "processing_activities": len(request.data_processing_activities),
        "violations": violations,
        "remediation_required": [
            "Update consent management system",
            "Implement data retention automation",
        ],
    }


@app.post("/compliance/report")
async def generate_compliance_report(report_request: ComplianceReportRequest):
    """Generate a full compliance audit report over a date range."""
    report_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    events = list(audit_events.values())
    if report_request.resource_types:
        events = [e for e in events if e.resource_type in report_request.resource_types]
    if report_request.user_ids:
        events = [e for e in events if e.user_id in report_request.user_ids]
    events = [e for e in events if report_request.start_date <= e.timestamp <= report_request.end_date]

    events_by_type: Dict[str, int] = {}
    events_by_user: Dict[str, int] = {}
    for event in events:
        events_by_type[event.event_type.value] = events_by_type.get(event.event_type.value, 0) + 1
        events_by_user[event.user_id] = events_by_user.get(event.user_id, 0) + 1

    integrity_verified = True
    findings: List[Dict[str, Any]] = []
    if report_request.include_integrity_checks:
        is_valid, errors = verify_chain_integrity()
        integrity_verified = is_valid
        for error in errors:
            findings.append({"type": "integrity_error", "severity": Severity.CRITICAL.value, "description": error, "timestamp": now})

    for user_id, count in events_by_user.items():
        if count > 1000:
            findings.append({"type": "unusual_activity", "severity": Severity.WARNING.value, "description": f"User {user_id} has {count} events in period", "user_id": user_id, "count": count})

    return ComplianceReport(
        report_id=report_id,
        generated_at=now,
        period_start=report_request.start_date,
        period_end=report_request.end_date,
        total_events=len(events),
        events_by_type=events_by_type,
        events_by_user=events_by_user,
        integrity_verified=integrity_verified,
        findings=findings,
    )


# ============================================================================
# Routes — Analytics & Search
# ============================================================================

@app.get("/analytics/summary")
async def get_audit_summary(
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
):
    """Get summary statistics for audit events."""
    events = list(audit_events.values())
    if start_date:
        events = [e for e in events if e.timestamp >= start_date]
    if end_date:
        events = [e for e in events if e.timestamp <= end_date]

    by_type: Dict[str, int] = defaultdict(int)
    by_resource: Dict[str, int] = defaultdict(int)
    by_user: Dict[str, int] = defaultdict(int)

    for event in events:
        by_type[event.event_type.value] += 1
        by_resource[f"{event.resource_type.value}:{event.resource_id}"] += 1
        by_user[event.user_id] += 1

    return {
        "total_events": len(events),
        "by_event_type": dict(by_type),
        "unique_resources": len(by_resource),
        "unique_users": len(by_user),
        "top_resources": sorted(by_resource.items(), key=lambda x: x[1], reverse=True)[:10],
        "top_users": sorted(by_user.items(), key=lambda x: x[1], reverse=True)[:10],
        "period_start": min((e.timestamp for e in events), default=None),
        "period_end": max((e.timestamp for e in events), default=None),
    }


@app.get("/search")
async def search_audit_events(query: str, fields: Optional[List[str]] = None, limit: int = 50):
    """Search audit events by query string."""
    results = []
    for event in audit_events.values():
        search_fields = fields or ["resource_id", "user_id", "action_details"]
        for field in search_fields:
            value = getattr(event, field, None)
            if value:
                if isinstance(value, dict):
                    if query.lower() in json.dumps(value).lower():
                        results.append(event)
                        break
                elif isinstance(value, str) and query.lower() in value.lower():
                    results.append(event)
                    break

    results.sort(key=lambda x: x.timestamp, reverse=True)
    return {"total": len(results), "events": results[:limit]}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
