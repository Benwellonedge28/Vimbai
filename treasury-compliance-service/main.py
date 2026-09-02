"""Vimbai Treasury Compliance Service - Compliance monitoring for treasury operations. Port: 8321"""

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

SERVICE_NAME = "treasury-compliance-service"
PORT = int(os.getenv("PORT", "8321"))
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
app = FastAPI(title="Vimbai Treasury Compliance Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="treasury-compliance-service", instrument_app=app)
except ImportError:
    TRACER = None


class ComplianceStatus(str, Enum):
    COMPLIANT = "compliant"
    WARNING = "warning"
    NON_COMPLIANT = "non_compliant"
    PENDING_REVIEW = "pending_review"


class ComplianceCheck(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    check_name: str
    regulation: str
    status: ComplianceStatus = ComplianceStatus.PENDING_REVIEW
    details: str = ""
    checked_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    remediation: str = ""


DEFAULT_CHECKS = [
    {
        "check_name": "Counterparty Limit Compliance",
        "regulation": "Basel III",
        "description": "Ensure counterparty exposure is within regulatory limits",
    },
    {"check_name": "Liquidity Coverage Ratio", "regulation": "Basel III LCR", "description": "Maintain LCR above 100%"},
    {
        "check_name": "FX Exposure Limits",
        "regulation": "Internal Policy",
        "description": "Verify foreign exchange exposure within approved limits",
    },
    {
        "check_name": "Investment Guidelines",
        "regulation": "Board Policy",
        "description": "Ensure investments comply with board-approved guidelines",
    },
    {
        "check_name": "Segregation of Duties",
        "regulation": "SOX",
        "description": "Verify treasury duties are properly segregated",
    },
    {
        "check_name": "Reporting Timeliness",
        "regulation": "Regulatory",
        "description": "Ensure regulatory reports submitted on time",
    },
]
_checks: Dict[str, List[ComplianceCheck]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.get("/checks/{company_id}")
async def get_compliance_checks(company_id: str):
    checks = _checks.get(company_id, [])
    if not checks:
        for c in DEFAULT_CHECKS:
            checks.append(
                ComplianceCheck(
                    company_id=company_id,
                    check_name=c["check_name"],
                    regulation=c["regulation"],
                    details=c["description"],
                )
            )
        _checks[company_id] = checks
    compliant = sum(1 for c in checks if c.status == ComplianceStatus.COMPLIANT)
    return {
        "company_id": company_id,
        "checks": checks,
        "total": len(checks),
        "compliant": compliant,
        "compliance_rate": compliant / max(1, len(checks)),
    }


@app.put("/checks/{check_id}/status")
async def update_check_status(check_id: str, status: ComplianceStatus, remediation: str = ""):
    for checks in _checks.values():
        for c in checks:
            if c.id == check_id:
                c.status = status
                if remediation:
                    c.remediation = remediation
                return {"check_id": check_id, "status": status.value}
    raise HTTPException(status_code=404, detail="Check not found")


@app.get("/report/{company_id}")
async def compliance_report(company_id: str):
    checks = _checks.get(company_id, [])
    if not checks:
        return {
            "company_id": company_id,
            "compliance_rate": 1.0,
            "findings": [],
            "recommendations": ["Run compliance checks first"],
        }
    findings = [c for c in checks if c.status in (ComplianceStatus.WARNING, ComplianceStatus.NON_COMPLIANT)]
    return {
        "company_id": company_id,
        "compliance_rate": sum(1 for c in checks if c.status == ComplianceStatus.COMPLIANT) / len(checks),
        "total_findings": len(findings),
        "findings": findings,
        "recommendations": [f.remediation for f in findings if f.remediation],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
