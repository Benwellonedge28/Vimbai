"""Pydantic models and enums for Audit Compliance Service."""

from datetime import date, datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel


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
