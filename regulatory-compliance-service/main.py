"""
Vimbai Regulatory Compliance Service
Multi-jurisdiction regulatory requirement tracking and compliance monitoring.
Port: 8396
"""

import os
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "regulatory-compliance-service"
PORT = int(os.getenv("PORT", "8396"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Regulatory Compliance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class RegStatus(str, Enum):
    COMPLIANT = "compliant"
    NON_COMPLIANT = "non_compliant"
    PENDING_REVIEW = "pending_review"
    NOT_APPLICABLE = "n/a"


class Regulation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    regulation_name: str
    jurisdiction: str
    framework: str  # iFRS, IAS, SOX, Basel III, AML, GDPR, PCI_DSS
    requirement: str
    status: RegStatus = RegStatus.PENDING_REVIEW
    last_reviewed: str = ""
    next_review_due: str = ""
    risk_if_non_compliant: str = "medium"  # low, medium, high, critical


class ComplianceDashboard(BaseModel):
    company_id: str
    total_regulations: int
    compliant: int
    non_compliant: int
    pending: int
    compliance_rate: float
    by_framework: Dict[str, Dict] = {}
    by_jurisdiction: Dict[str, Dict] = {}
    critical_items: List[Dict] = []


_regulations: Dict[str, List[Regulation]] = {}


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/regulations", response_model=Regulation)
async def add_regulation(reg: Regulation):
    _regulations.setdefault(reg.company_id, []).append(reg)
    return reg


@app.get("/regulations", response_model=List[Regulation])
async def list_regulations(company_id: str, framework: str = ""):
    items = _regulations.get(company_id, [])
    if framework:
        items = [r for r in items if r.framework == framework]
    return items


@app.post("/regulations/{reg_id}/update")
async def update_reg_status(company_id: str, reg_id: str, status: str):
    items = _regulations.get(company_id, [])
    for r in items:
        if r.id == reg_id:
            r.status = RegStatus(status) if status in [s.value for s in RegStatus] else r.status
            r.last_reviewed = datetime.now(timezone.utc).strftime("%Y-%m-%d")
            return {"updated": True, "regulation_id": reg_id, "status": r.status.value}
    from fastapi import HTTPException

    raise HTTPException(status_code=404, detail="Regulation not found")


@app.get("/dashboard", response_model=ComplianceDashboard)
async def get_dashboard(company_id: str):
    items = _regulations.get(company_id, [])
    compliant = sum(1 for r in items if r.status == RegStatus.COMPLIANT)
    non_compliant = sum(1 for r in items if r.status == RegStatus.NON_COMPLIANT)
    pending = sum(1 for r in items if r.status == RegStatus.PENDING_REVIEW)
    total = len(items)
    rate = (compliant / total * 100) if total else 100

    by_framework = {}
    for r in items:
        fw = r.framework
        if fw not in by_framework:
            by_framework[fw] = {"total": 0, "compliant": 0, "non_compliant": 0}
        by_framework[fw]["total"] += 1
        if r.status == RegStatus.COMPLIANT:
            by_framework[fw]["compliant"] += 1
        elif r.status == RegStatus.NON_COMPLIANT:
            by_framework[fw]["non_compliant"] += 1

    by_jur = {}
    for r in items:
        j = r.jurisdiction
        if j not in by_jur:
            by_jur[j] = {"total": 0, "compliant": 0}
        by_jur[j]["total"] += 1
        if r.status == RegStatus.COMPLIANT:
            by_jur[j]["compliant"] += 1

    critical = [
        {
            "regulation": r.regulation_name,
            "jurisdiction": r.jurisdiction,
            "framework": r.framework,
            "status": r.status.value,
            "risk": r.risk_if_non_compliant,
        }
        for r in items
        if r.risk_if_non_compliant in ("high", "critical") and r.status != RegStatus.COMPLIANT
    ]

    return ComplianceDashboard(
        company_id=company_id,
        total_regulations=total,
        compliant=compliant,
        non_compliant=non_compliant,
        pending=pending,
        compliance_rate=round(rate, 1),
        by_framework=by_framework,
        by_jurisdiction=by_jur,
        critical_items=critical,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
