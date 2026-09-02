"""
Vimbai Regulatory Reporting Service
Automated regulatory report generation for central bank and securities authority filings.
Port: 8407
"""

import os
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "regulatory-reporting-service"
PORT = int(os.getenv("PORT", "8407"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Regulatory Reporting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class ReportType(str, Enum):
    PRUDENTIAL = "prudential"
    LIQUIDITY = "liquidity"
    CAPITAL_ADEQUACY = "capital_adequacy"
    LARGE_EXPOSURES = "large_exposures"
    RELATED_PARTY = "related_party"
    AML = "aml"
    FX_EXPOSURE = "fx_exposure"


class ReportRequest(BaseModel):
    company_id: str
    report_type: ReportType
    period: str
    jurisdiction: str = "ZW"
    data: Dict[str, float] = {}


class ReportResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    report_type: str
    period: str
    jurisdiction: str
    status: str
    filing_reference: str
    summary: Dict[str, float]
    validation_checks: List[Dict] = []
    submission_deadline: str


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/generate", response_model=ReportResult)
async def generate_report(req: ReportRequest):
    filing_ref = f"REG-{req.jurisdiction}-{req.report_type.value.upper()}-{req.period.replace('-', '')}"

    validation = []
    all_pass = True

    if req.report_type == ReportType.PRUDENTIAL:
        car = req.data.get("capital_ratio", 0)
        validation.append({"check": "Capital Adequacy Ratio >= 12%", "value": car, "pass": car >= 12})
        if car < 12:
            all_pass = False

    elif req.report_type == ReportType.LIQUIDITY:
        lcr = req.data.get("liquidity_ratio", 0)
        validation.append({"check": "Liquidity Coverage Ratio >= 100%", "value": lcr, "pass": lcr >= 100})
        if lcr < 100:
            all_pass = False

    elif req.report_type == ReportType.LARGE_EXPOSURES:
        max_exp = req.data.get("largest_exposure_pct", 0)
        validation.append({"check": "Single exposure <= 25% of capital", "value": max_exp, "pass": max_exp <= 25})
        if max_exp > 25:
            all_pass = False

    validation.append({"check": "Data completeness", "value": len(req.data), "pass": len(req.data) > 0})
    if not req.data:
        all_pass = False

    status = "ready_for_submission" if all_pass else "validation_failed"

    from datetime import timedelta

    deadline = (datetime.now(timezone.utc) + timedelta(days=30)).strftime("%Y-%m-%d")

    return ReportResult(
        company_id=req.company_id,
        report_type=req.report_type.value,
        period=req.period,
        jurisdiction=req.jurisdiction,
        status=status,
        filing_reference=filing_ref,
        summary={k: round(v, 2) for k, v in req.data.items()},
        validation_checks=validation,
        submission_deadline=deadline,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
