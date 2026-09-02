"""
Vimbai Report Automation Service
Automates report generation schedules and template-based report creation.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "report-automation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8438"))

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

app = FastAPI(title="Vimbai Report Automation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class ReportTemplate(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    report_type: str  # financial_summary, kpi_dashboard, compliance, operational
    description: str = ""
    sections: List[str] = []
    data_sources: List[str] = []
    format: str = "pdf"  # pdf, xlsx, csv, html
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ReportSchedule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    template_id: str
    name: str
    cron_expression: str  # e.g. "0 0 1 * *" for monthly on 1st
    recipients: List[str] = []
    status: str = "active"  # active, paused, completed
    last_run: Optional[datetime] = None
    next_run: Optional[datetime] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class GeneratedReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    schedule_id: str
    template_id: str
    period: str
    data: Dict[str, Any] = {}
    status: str = "generated"  # generated, failed, delivered
    generated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    delivered_to: List[str] = []


templates: List[ReportTemplate] = []
schedules: List[ReportSchedule] = []
generated_reports: List[GeneratedReport] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/templates", response_model=ReportTemplate)
async def create_template(
    name: str,
    report_type: str,
    description: str = "",
    sections: List[str] = [],
    data_sources: List[str] = [],
    format: str = "pdf",
):
    """Create a report template."""
    valid_types = ["financial_summary", "kpi_dashboard", "compliance", "operational"]
    if report_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid report type. Must be one of {valid_types}")

    template = ReportTemplate(
        name=name,
        report_type=report_type,
        description=description,
        sections=sections,
        data_sources=data_sources,
        format=format,
    )
    templates.append(template)
    logger.info("Report template created", template_id=template.id, name=name)
    return template


@app.get("/templates", response_model=List[ReportTemplate])
async def list_templates(report_type: Optional[str] = None):
    """List report templates."""
    if report_type:
        return [t for t in templates if t.report_type == report_type]
    return templates


@app.post("/schedules", response_model=ReportSchedule)
async def create_schedule(template_id: str, name: str, cron_expression: str, recipients: List[str] = []):
    """Create a report generation schedule."""
    template = next((t for t in templates if t.id == template_id), None)
    if not template:
        raise HTTPException(status_code=404, detail="Template not found")

    schedule = ReportSchedule(
        template_id=template_id,
        name=name,
        cron_expression=cron_expression,
        recipients=recipients,
        next_run=datetime.now(timezone.utc),
    )
    schedules.append(schedule)
    logger.info("Report schedule created", schedule_id=schedule.id, name=name, cron=cron_expression)
    return schedule


@app.get("/schedules", response_model=List[ReportSchedule])
async def list_schedules(status: Optional[str] = None):
    """List report schedules."""
    if status:
        return [s for s in schedules if s.status == status]
    return schedules


@app.post("/schedules/{schedule_id}/run", response_model=GeneratedReport)
async def run_report(schedule_id: str, period: str = ""):
    """Manually trigger a report generation."""
    schedule = next((s for s in schedules if s.id == schedule_id), None)
    if not schedule:
        raise HTTPException(status_code=404, detail="Schedule not found")
    if schedule.status != "active":
        raise HTTPException(status_code=400, detail="Schedule is not active")

    template = next((t for t in templates if t.id == schedule.template_id), None)

    report = GeneratedReport(
        schedule_id=schedule_id,
        template_id=schedule.template_id,
        period=period or datetime.now(timezone.utc).strftime("%Y-%m"),
        data={"sections": template.sections if template else []},
        delivered_to=schedule.recipients,
        status="delivered" if schedule.recipients else "generated",
    )
    generated_reports.append(report)
    schedule.last_run = datetime.now(timezone.utc)
    logger.info("Report generated", schedule_id=schedule_id, report_id=report.id, period=period)
    return report


@app.get("/reports", response_model=List[GeneratedReport])
async def list_reports(schedule_id: Optional[str] = None, limit: int = 50):
    """List generated reports."""
    result = generated_reports
    if schedule_id:
        result = [r for r in result if r.schedule_id == schedule_id]
    return result[-limit:]


@app.put("/schedules/{schedule_id}/pause")
async def pause_schedule(schedule_id: str):
    """Pause a report schedule."""
    schedule = next((s for s in schedules if s.id == schedule_id), None)
    if not schedule:
        raise HTTPException(status_code=404, detail="Schedule not found")
    schedule.status = "paused"
    return {"schedule_id": schedule_id, "status": "paused"}


@app.put("/schedules/{schedule_id}/resume")
async def resume_schedule(schedule_id: str):
    """Resume a paused report schedule."""
    schedule = next((s for s in schedules if s.id == schedule_id), None)
    if not schedule:
        raise HTTPException(status_code=404, detail="Schedule not found")
    schedule.status = "active"
    return {"schedule_id": schedule_id, "status": "active"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
