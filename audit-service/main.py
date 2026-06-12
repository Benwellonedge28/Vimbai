"""
FinAcc Audit Service
Immutable Audit Trail & Financial Data Versioning
Implements tamper-proof event logging with graph-native data lineage
"""

from fastapi import FastAPI, HTTPException, Depends, status, Request, BackgroundTasks
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any, Literal
from datetime import datetime, timezone, timedelta
from enum import Enum
import uuid
import hashlib
import json
from collections import defaultdict

app = FastAPI(
    title="FinAcc Audit Service",
    description="Immutable Audit Trail & Financial Data Versioning with Graph-Native Data Lineage",
    version="1.0.0",
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
# Pydantic Models
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
    previous_event_id: Optional[str] = None
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

# ============================================================================
# In-Memory Storage (In production, use Neo4j Graph Database)
# ============================================================================

audit_events: Dict[str, AuditEvent] = {}
version_snapshots: Dict[str, List[VersionSnapshot]] = defaultdict(list)
data_lineage: Dict[str, List[str]] = defaultdict(list)  # resource_id -> event_ids
integrity_chain: List[str] = []  # Ordered list of event IDs for chain verification

# ============================================================================
# Helper Functions
# ============================================================================

def generate_checksum(event_data: Dict) -> str:
    """Generate SHA-256 checksum for event data integrity verification"""
    # Create deterministic JSON string for hashing
    content = json.dumps(event_data, sort_keys=True, default=str)
    return hashlib.sha256(content.encode()).hexdigest()

def verify_chain_integrity() -> tuple[bool, List[str]]:
    """Verify the integrity of the audit chain"""
    errors = []
    for i, event_id in enumerate(integrity_chain):
        event = audit_events.get(event_id)
        if not event:
            errors.append(f"Event {event_id} not found at position {i}")
            continue

        # Verify checksum
        computed_checksum = generate_checksum(event.model_dump(exclude={'checksum'}))
        if computed_checksum != event.checksum:
            errors.append(f"Checksum mismatch for event {event_id}")

        # Verify chain linkage
        if i > 0:
            previous_event = audit_events.get(integrity_chain[i - 1])
            if previous_event and event.previous_event_id != previous_event.id:
                errors.append(f"Chain broken at position {i}: event {event_id} doesn't reference previous event")

    return len(errors) == 0, errors

def calculate_hash_chain(event_ids: List[str]) -> str:
    """Calculate hash chain for a sequence of events"""
    chain_hash = ""
    for event_id in event_ids:
        event = audit_events.get(event_id)
        if event:
            chain_hash = hashlib.sha256((chain_hash + event.checksum).encode()).hexdigest()
    return chain_hash

# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    integrity_ok, _ = verify_chain_integrity()
    return {
        "status": "healthy",
        "service": "audit",
        "version": "1.0.0",
        "total_events": len(audit_events),
        "total_snapshots": sum(len(v) for v in version_snapshots.values()),
        "chain_integrity": "verified" if integrity_ok else "broken"
    }

# --- Event Logging ---

@app.post("/events", status_code=status.HTTP_201_CREATED)
async def create_audit_event(event_data: AuditEventCreate, request: Request):
    """Create a new immutable audit event"""
    event_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    # Get previous event for chain linking
    previous_event_id = integrity_chain[-1] if integrity_chain else None

    # Build event data for checksum
    event_dict = {
        "event_type": event_data.event_type.value,
        "resource_type": event_data.resource_type.value,
        "resource_id": event_data.resource_id,
        "user_id": event_data.user_id,
        "user_email": event_data.user_email,
        "organization_id": event_data.organization_id,
        "action_details": event_data.action_details,
        "previous_state": event_data.previous_state,
        "new_state": event_data.new_state,
        "metadata": event_data.metadata,
        "previous_event_id": previous_event_id,
        "timestamp": now.isoformat(),
        "ip_address": request.client.host if request else None,
        "user_agent": request.headers.get("user-agent") if request else None,
        "session_id": event_data.session_id,
        "correlation_id": event_data.correlation_id
    }

    checksum = generate_checksum(event_dict)

    event = AuditEvent(
        id=event_id,
        event_type=event_data.event_type,
        resource_type=event_data.resource_type,
        resource_id=event_data.resource_id,
        user_id=event_data.user_id,
        user_email=event_data.user_email,
        organization_id=event_data.organization_id,
        action_details=event_data.action_details,
        previous_state=event_data.previous_state,
        new_state=event_data.new_state,
        metadata=event_data.metadata,
        checksum=checksum,
        previous_event_id=previous_event_id,
        timestamp=now,
        ip_address=request.client.host if request else None,
        user_agent=request.headers.get("user-agent") if request else None,
        session_id=event_data.session_id,
        correlation_id=event_data.correlation_id
    )

    audit_events[event_id] = event
    integrity_chain.append(event_id)
    data_lineage[event_data.resource_id].append(event_id)

    return {
        "id": event_id,
        "checksum": checksum,
        "timestamp": now,
        "chain_position": len(integrity_chain)
    }

@app.post("/events/batch")
async def create_audit_events_batch(
    events: List[AuditEventCreate],
    background_tasks: BackgroundTasks,
    request: Request = None
):
    """Create multiple audit events in batch"""
    results = []

    for event_data in events:
        result = await create_audit_event(event_data, request)
        results.append(result)

    return {
        "created": len(results),
        "events": results
    }

@app.get("/events/{event_id}")
async def get_audit_event(event_id: str):
    """Get a specific audit event"""
    if event_id not in audit_events:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Audit event not found")
    return audit_events[event_id]

@app.get("/events")
async def list_audit_events(
    resource_type: Optional[ResourceType] = None,
    resource_id: Optional[str] = None,
    event_type: Optional[EventType] = None,
    user_id: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    offset: int = 0
):
    """List audit events with filters"""
    results = list(audit_events.values())

    if resource_type:
        results = [e for e in results if e.resource_type == resource_type]
    if resource_id:
        results = [e for e in results if e.resource_id == resource_id]
    if event_type:
        results = [e for e in results if e.event_type == event_type]
    if user_id:
        results = [e for e in results if e.user_id == user_id]
    if start_date:
        results = [e for e in results if e.timestamp >= start_date]
    if end_date:
        results = [e for e in results if e.timestamp <= end_date]

    results.sort(key=lambda x: x.timestamp, reverse=True)
    total = len(results)
    results = results[offset:offset + limit]

    return {"total": total, "events": results}

# --- Resource History ---

@app.get("/resources/{resource_type}/{resource_id}/history")
async def get_resource_history(
    resource_type: ResourceType,
    resource_id: str,
    include_states: bool = True
):
    """Get complete history of changes for a resource"""
    event_ids = data_lineage.get(resource_id, [])

    if not event_ids:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="No history found for this resource")

    events = [audit_events[eid] for eid in event_ids if eid in audit_events]

    history = []
    for event in events:
        entry = {
            "event_id": event.id,
            "event_type": event.event_type.value,
            "timestamp": event.timestamp,
            "user_id": event.user_id,
            "action_details": event.action_details
        }
        if include_states:
            entry["previous_state"] = event.previous_state
            entry["new_state"] = event.new_state

        history.append(entry)

    return {
        "resource_type": resource_type.value,
        "resource_id": resource_id,
        "total_changes": len(history),
        "history": history
    }

@app.get("/resources/{resource_type}/{resource_id}/versions")
async def get_resource_versions(
    resource_type: ResourceType,
    resource_id: str
):
    """Get version snapshots for a resource"""
    snapshots = version_snapshots.get(resource_id, [])

    return {
        "resource_type": resource_type.value,
        "resource_id": resource_id,
        "total_versions": len(snapshots),
        "versions": [
            {
                "version": s.version,
                "state": s.state,
                "changed_by": s.changed_by,
                "changed_at": s.changed_at,
                "change_reason": s.change_reason
            }
            for s in snapshots
        ]
    }

@app.post("/resources/{resource_type}/{resource_id}/versions")
async def create_version_snapshot(
    resource_type: ResourceType,
    resource_id: str,
    state: Dict[str, Any],
    changed_by: str,
    change_reason: Optional[str] = None
):
    """Create a version snapshot for a resource"""
    snapshots = version_snapshots[resource_id]
    version_number = len(snapshots) + 1

    version_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    version_data = {
        "resource_type": resource_type.value,
        "resource_id": resource_id,
        "version": version_number,
        "state": state,
        "changed_by": changed_by,
        "changed_at": now.isoformat(),
        "change_reason": change_reason
    }
    checksum = generate_checksum(version_data)

    snapshot = VersionSnapshot(
        id=version_id,
        resource_type=resource_type,
        resource_id=resource_id,
        version=version_number,
        state=state,
        changed_by=changed_by,
        changed_at=now,
        change_reason=change_reason,
        checksum=checksum
    )

    snapshots.append(snapshot)

    return {
        "id": version_id,
        "version": version_number,
        "checksum": checksum,
        "created_at": now
    }

@app.get("/resources/{resource_type}/{resource_id}/versions/{version}")
async def get_version_at(
    resource_type: ResourceType,
    resource_id: str,
    version: int
):
    """Get a specific version of a resource"""
    snapshots = version_snapshots.get(resource_id, [])

    for snapshot in snapshots:
        if snapshot.version == version:
            return snapshot

    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Version not found")

# --- Data Lineage ---

@app.get("/lineage/{resource_id}")
async def get_data_lineage(
    resource_id: str,
    depth: int = 10,
    direction: Literal["forward", "backward", "both"] = "both"
):
    """Trace data lineage for a resource"""
    event_ids = data_lineage.get(resource_id, [])

    if not event_ids:
        return {
            "resource_id": resource_id,
            "lineage": [],
            "message": "No lineage data found"
        }

    nodes = []
    edges = []

    # Build lineage graph
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
            source_event_id=event.correlation_id
        )
        nodes.append(node)

        # Create edges to related events
        if event.correlation_id and event.correlation_id in audit_events:
            related_event = audit_events[event.correlation_id]
            edge = DataLineageEdge(
                id=str(uuid.uuid4()),
                from_node_id=event.correlation_id,
                to_node_id=event_id,
                relationship_type="caused_by"
            )
            edges.append(edge)

    return {
        "resource_id": resource_id,
        "nodes": [n.model_dump() for n in nodes],
        "edges": [e.model_dump() for e in edges],
        "total_events": len(event_ids),
        "displayed_events": len(nodes)
    }

# --- Integrity Verification ---

@app.get("/integrity/verify")
async def verify_integrity():
    """Verify the integrity of the entire audit chain"""
    is_valid, errors = verify_chain_integrity()

    chain_hash = calculate_hash_chain(integrity_chain)

    return {
        "is_valid": is_valid,
        "total_events": len(audit_events),
        "chain_length": len(integrity_chain),
        "chain_hash": chain_hash,
        "errors": errors,
        "verified_at": datetime.now(timezone.utc)
    }

@app.get("/integrity/verify-event/{event_id}")
async def verify_single_event(event_id: str):
    """Verify integrity of a single event"""
    if event_id not in audit_events:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Event not found")

    event = audit_events[event_id]
    computed_checksum = generate_checksum(event.model_dump(exclude={'checksum'}))

    return {
        "event_id": event_id,
        "stored_checksum": event.checksum,
        "computed_checksum": computed_checksum,
        "matches": computed_checksum == event.checksum,
        "verified_at": datetime.now(timezone.utc)
    }

@app.get("/integrity/verify-range")
async def verify_event_range(
    start_event_id: str,
    end_event_id: str
):
    """Verify integrity of a range of events"""
    if start_event_id not in integrity_chain or end_event_id not in integrity_chain:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Event IDs not in chain")

    start_idx = integrity_chain.index(start_event_id)
    end_idx = integrity_chain.index(end_event_id)

    if start_idx > end_idx:
        start_idx, end_idx = end_idx, start_idx

    range_events = integrity_chain[start_idx:end_idx + 1]
    range_hash = calculate_hash_chain(range_events)

    return {
        "start_event_id": start_event_id,
        "end_event_id": end_event_id,
        "event_count": len(range_events),
        "range_hash": range_hash,
        "verified_at": datetime.now(timezone.utc)
    }

# --- Compliance Reports ---

@app.post("/compliance/report")
async def generate_compliance_report(report_request: ComplianceReportRequest):
    """Generate a compliance audit report"""
    report_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    # Filter events
    events = list(audit_events.values())

    if report_request.resource_types:
        events = [e for e in events if e.resource_type in report_request.resource_types]

    if report_request.user_ids:
        events = [e for e in events if e.user_id in report_request.user_ids]

    events = [e for e in events if report_request.start_date <= e.timestamp <= report_request.end_date]

    # Count by type
    events_by_type = {}
    events_by_user = {}

    for event in events:
        type_key = event.event_type.value
        events_by_type[type_key] = events_by_type.get(type_key, 0) + 1

        events_by_user[event.user_id] = events_by_user.get(event.user_id, 0) + 1

    # Verify integrity
    integrity_verified = True
    findings = []

    if report_request.include_integrity_checks:
        is_valid, errors = verify_chain_integrity()
        integrity_verified = is_valid

        if not is_valid:
            for error in errors:
                findings.append({
                    "type": "integrity_error",
                    "severity": Severity.CRITICAL.value,
                    "description": error,
                    "timestamp": now
                })

    # Check for suspicious patterns
    for user_id, count in events_by_user.items():
        if count > 1000:  # Unusually high activity
            findings.append({
                "type": "unusual_activity",
                "severity": Severity.WARNING.value,
                "description": f"User {user_id} has {count} events in period",
                "user_id": user_id,
                "count": count
            })

    report = ComplianceReport(
        report_id=report_id,
        generated_at=now,
        period_start=report_request.start_date,
        period_end=report_request.end_date,
        total_events=len(events),
        events_by_type=events_by_type,
        events_by_user=events_by_user,
        integrity_verified=integrity_verified,
        findings=findings
    )

    return report

# --- Search & Analytics ---

@app.get("/analytics/summary")
async def get_audit_summary(
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None
):
    """Get summary statistics for audit events"""
    events = list(audit_events.values())

    if start_date:
        events = [e for e in events if e.timestamp >= start_date]
    if end_date:
        events = [e for e in events if e.timestamp <= end_date]

    by_type = defaultdict(int)
    by_resource = defaultdict(int)
    by_user = defaultdict(int)

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
        "period_end": max((e.timestamp for e in events), default=None)
    }

@app.get("/search")
async def search_audit_events(
    query: str,
    fields: Optional[List[str]] = None,
    limit: int = 50
):
    """Search audit events by query string"""
    results = []

    for event in audit_events.values():
        match = False

        # Search in common fields
        search_fields = fields or ["resource_id", "user_id", "action_details"]

        for field in search_fields:
            value = getattr(event, field, None)
            if value:
                if isinstance(value, dict):
                    value_str = json.dumps(value)
                    if query.lower() in value_str.lower():
                        match = True
                        break
                elif isinstance(value, str) and query.lower() in value.lower():
                    match = True
                    break

        if match:
            results.append(event)

    results.sort(key=lambda x: x.timestamp, reverse=True)
    return {"total": len(results), "events": results[:limit]}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8091)