"""
Vimbai Report Automation Service
Automates report generation schedules and template-based report creation.
"""

import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "report-automation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8272"))

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

# Loaded bare (uvicorn main:app) and by path (root integration tests), so this
# directory has to be importable before the store module is pulled in.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import report_automation_store as store  # noqa: E402

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the caller and Book context (set by the gateway) to the store."""
    store.user_id_var.set(request.headers.get("X-User-Id"))
    store.book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


# Templates, schedules and generated reports are durable records: Neo4j,
# caller-owned, Book-gated.
TEMPLATES = ("ReportTemplate", "OWNS_REPORT_TEMPLATE")
SCHEDULES = ("ReportSchedule", "OWNS_REPORT_SCHEDULE")
REPORTS = ("GeneratedReport", "OWNS_GENERATED_REPORT")


def _template_model(node: Dict[str, Any]) -> ReportTemplate:
    return ReportTemplate(
        id=node["id"],
        name=node.get("name", ""),
        report_type=node.get("report_type", ""),
        description=node.get("description", ""),
        sections=store.loads(node.get("sections_json")),
        data_sources=store.loads(node.get("data_sources_json")),
        format=node.get("format", "pdf"),
        created_at=node.get("created_at"),
    )


def _schedule_model(node: Dict[str, Any]) -> ReportSchedule:
    return ReportSchedule(
        id=node["id"],
        template_id=node.get("template_id", ""),
        name=node.get("name", ""),
        cron_expression=node.get("cron_expression", ""),
        recipients=store.loads(node.get("recipients_json")),
        status=node.get("status", "active"),
        last_run=node.get("last_run"),
        next_run=node.get("next_run"),
        created_at=node.get("created_at"),
    )


def _report_model(node: Dict[str, Any]) -> GeneratedReport:
    return GeneratedReport(
        id=node["id"],
        schedule_id=node.get("schedule_id", ""),
        template_id=node.get("template_id", ""),
        period=node.get("period", ""),
        data=store.loads(node.get("data_json")) if node.get("data_json") else {},
        status=node.get("status", "generated"),
        generated_at=node.get("generated_at"),
        delivered_to=store.loads(node.get("delivered_to_json")),
    )


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
    node = await store.create_node(
        TEMPLATES[0],
        TEMPLATES[1],
        {
            "id": template.id,
            "name": template.name,
            "report_type": template.report_type,
            "description": template.description,
            "sections_json": store.dumps(template.sections),
            "data_sources_json": store.dumps(template.data_sources),
            "format": template.format,
            "created_at": template.created_at.isoformat(),
        },
    )
    logger.info("Report template created", template_id=template.id, name=name)
    return _template_model(node)


@app.get("/templates", response_model=List[ReportTemplate])
async def list_templates(report_type: Optional[str] = None):
    """List report templates."""
    nodes = await store.list_nodes(TEMPLATES[0], TEMPLATES[1], {"report_type": report_type})
    return [_template_model(node) for node in nodes]


@app.post("/schedules", response_model=ReportSchedule)
async def create_schedule(template_id: str, name: str, cron_expression: str, recipients: List[str] = []):
    """Create a report generation schedule."""
    template_node = await store.get_node(TEMPLATES[0], TEMPLATES[1], template_id)
    if not template_node:
        raise HTTPException(status_code=404, detail="Template not found")

    schedule = ReportSchedule(
        template_id=template_id,
        name=name,
        cron_expression=cron_expression,
        recipients=recipients,
        next_run=datetime.now(timezone.utc),
    )
    node = await store.create_node(
        SCHEDULES[0],
        SCHEDULES[1],
        {
            "id": schedule.id,
            "template_id": schedule.template_id,
            "name": schedule.name,
            "cron_expression": schedule.cron_expression,
            "recipients_json": store.dumps(schedule.recipients),
            "status": schedule.status,
            "last_run": None,
            "next_run": schedule.next_run.isoformat(),
            "created_at": schedule.created_at.isoformat(),
        },
    )
    logger.info("Report schedule created", schedule_id=schedule.id, name=name, cron=cron_expression)
    return _schedule_model(node)


@app.get("/schedules", response_model=List[ReportSchedule])
async def list_schedules(status: Optional[str] = None):
    """List report schedules."""
    nodes = await store.list_nodes(SCHEDULES[0], SCHEDULES[1], {"status": status})
    return [_schedule_model(node) for node in nodes]


@app.post("/schedules/{schedule_id}/run", response_model=GeneratedReport)
async def run_report(schedule_id: str, period: str = ""):
    """Manually trigger a report generation."""
    node = await store.get_node(SCHEDULES[0], SCHEDULES[1], schedule_id)
    if not node:
        raise HTTPException(status_code=404, detail="Schedule not found")
    schedule = _schedule_model(node)
    if schedule.status != "active":
        raise HTTPException(status_code=400, detail="Schedule is not active")

    template_node = await store.get_node(TEMPLATES[0], TEMPLATES[1], schedule.template_id)
    template = _template_model(template_node) if template_node else None

    report = GeneratedReport(
        schedule_id=schedule_id,
        template_id=schedule.template_id,
        period=period or datetime.now(timezone.utc).strftime("%Y-%m"),
        data={"sections": template.sections if template else []},
        delivered_to=schedule.recipients,
        status="delivered" if schedule.recipients else "generated",
    )
    report_node = await store.create_node(
        REPORTS[0],
        REPORTS[1],
        {
            "id": report.id,
            "schedule_id": report.schedule_id,
            "template_id": report.template_id,
            "period": report.period,
            "data_json": store.dumps(report.data),
            "status": report.status,
            "generated_at": report.generated_at.isoformat(),
            "delivered_to_json": store.dumps(report.delivered_to),
        },
    )
    await store.update_node(
        SCHEDULES[0], SCHEDULES[1], schedule_id, {"last_run": datetime.now(timezone.utc).isoformat()}
    )
    logger.info("Report generated", schedule_id=schedule_id, report_id=report.id, period=period)
    return _report_model(report_node)


@app.get("/reports", response_model=List[GeneratedReport])
async def list_reports(schedule_id: Optional[str] = None, limit: int = 50):
    """List generated reports."""
    nodes = await store.list_nodes(REPORTS[0], REPORTS[1], {"schedule_id": schedule_id})
    return [_report_model(node) for node in nodes][-limit:]


@app.put("/schedules/{schedule_id}/pause")
async def pause_schedule(schedule_id: str):
    """Pause a report schedule."""
    node = await store.get_node(SCHEDULES[0], SCHEDULES[1], schedule_id)
    if not node:
        raise HTTPException(status_code=404, detail="Schedule not found")
    await store.update_node(SCHEDULES[0], SCHEDULES[1], schedule_id, {"status": "paused"})
    return {"schedule_id": schedule_id, "status": "paused"}


@app.put("/schedules/{schedule_id}/resume")
async def resume_schedule(schedule_id: str):
    """Resume a paused report schedule."""
    node = await store.get_node(SCHEDULES[0], SCHEDULES[1], schedule_id)
    if not node:
        raise HTTPException(status_code=404, detail="Schedule not found")
    await store.update_node(SCHEDULES[0], SCHEDULES[1], schedule_id, {"status": "active"})
    return {"schedule_id": schedule_id, "status": "active"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
