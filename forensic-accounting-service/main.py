"""Vimbai Forensic Accounting Service - Audit and forensic analysis. Port: 8350"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "forensic-accounting-service"
PORT = int(os.getenv("PORT", "8350"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Forensic Accounting Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="forensic-accounting-service", instrument_app=app)
except ImportError:
    TRACER = None


class AuditStatus(str, Enum):
    PLANNED = "planned"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    CANCELLED = "cancelled"


class FindingSeverity(str, Enum):
    INFO = "info"
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class AuditFinding(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    title: str
    description: str
    severity: FindingSeverity = FindingSeverity.MEDIUM
    recommendation: str = ""
    status: str = "open"  # open, remediated, accepted
    evidence: str = ""


class AuditEngagement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    audit_type: str = "operational"
    title: str
    scope: str = ""
    objectives: List[str] = []
    start_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    end_date: Optional[datetime] = None
    auditor: str = ""
    status: AuditStatus = AuditStatus.PLANNED
    findings: List[AuditFinding] = []
    summary: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


_engagements: Dict[str, List[AuditEngagement]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/engagements", response_model=AuditEngagement)
async def create_engagement(engagement: AuditEngagement):
    _engagements[engagement.company_id].append(engagement)
    logger.info("engagement_created", company_id=engagement.company_id, type=engagement.audit_type)
    return engagement


@app.get("/engagements/{company_id}")
async def get_engagements(company_id: str, status_filter: Optional[str] = None):
    engs = _engagements.get(company_id, [])
    if status_filter:
        engs = [e for e in engs if e.status.value == status_filter]
    return {"company_id": company_id, "engagements": engs, "total": len(engs)}


@app.put("/engagements/{engagement_id}/status")
async def update_status(engagement_id: str, status: AuditStatus, summary: str = ""):
    for engs in _engagements.values():
        for e in engs:
            if e.id == engagement_id:
                e.status = status
                if status == AuditStatus.COMPLETED:
                    e.end_date = datetime.now(timezone.utc)
                    if summary:
                        e.summary = summary
                return {"id": engagement_id, "status": status.value}
    raise HTTPException(status_code=404, detail="Engagement not found")


@app.post("/engagements/{engagement_id}/findings")
async def add_finding(engagement_id: str, finding: AuditFinding):
    for engs in _engagements.values():
        for e in engs:
            if e.id == engagement_id:
                e.findings.append(finding)
                return {"engagement_id": engagement_id, "finding_id": finding.id, "severity": finding.severity.value}
    raise HTTPException(status_code=404, detail="Engagement not found")


@app.put("/findings/{finding_id}/remediate")
async def remediate_finding(finding_id: str, remediation_note: str = ""):
    for engs in _engagements.values():
        for e in engs:
            for f in e.findings:
                if f.id == finding_id:
                    f.status = "remediated"
                    if remediation_note:
                        f.recommendation = f"{f.recommendation}\n\nRemediation: {remediation_note}"
                    return {"finding_id": finding_id, "status": "remediated"}
    raise HTTPException(status_code=404, detail="Finding not found")


@app.get("/report/{engagement_id}")
async def audit_report(engagement_id: str):
    for engs in _engagements.values():
        for e in engs:
            if e.id == engagement_id:
                critical = sum(1 for f in e.findings if f.severity == FindingSeverity.CRITICAL)
                high = sum(1 for f in e.findings if f.severity == FindingSeverity.HIGH)
                medium = sum(1 for f in e.findings if f.severity == FindingSeverity.MEDIUM)
                low = sum(1 for f in e.findings if f.severity == FindingSeverity.LOW)
                return {
                    "engagement": e,
                    "findings_summary": {
                        "critical": critical,
                        "high": high,
                        "medium": medium,
                        "low": low,
                        "total": len(e.findings),
                    },
                    "open_findings": sum(1 for f in e.findings if f.status == "open"),
                    "remediated": sum(1 for f in e.findings if f.status == "remediated"),
                }
    raise HTTPException(status_code=404, detail="Engagement not found")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
