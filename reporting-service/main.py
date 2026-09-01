"""
Reporting Service
Port: 8361
Financial report generation and distribution
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Reporting Service", version="1.0.0")


# Distributed tracing
try:
    from shared.tracing import setup_tracing, get_tracer
    TRACER = setup_tracing(service_name="reporting-service", instrument_app=app)
except ImportError:
    TRACER = None
    import logging
    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")

class ReportRequest(BaseModel):
    company_id: str
    report_type: str
    period_start: date
    period_end: date
    format: str
    recipients: List[str]

class ReportResponse(BaseModel):
    report_id: str
    report_type: str
    status: str
    generated_at: datetime
    file_url: str
    recipients_notified: List[str]

class ScheduledReportRequest(BaseModel):
    company_id: str
    report_type: str
    schedule: str
    recipients: List[str]
    parameters: Dict[str, Any]

class ScheduledReportResponse(BaseModel):
    schedule_id: str
    report_type: str
    schedule: str
    next_run: datetime
    active: bool

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "reporting", "version": "1.0.0"}

@app.post("/generate", response_model=ReportResponse)
async def generate_report(request: ReportRequest):
    logger.info("Generating report", company=request.company_id, type=request.report_type)
    
    return ReportResponse(
        report_id=f"RPT-{datetime.now().strftime('%Y%m%d%H%M')}",
        report_type=request.report_type,
        status="generated",
        generated_at=datetime.now(),
        file_url=f"https://reports.example.com/{request.company_id}/{request.report_type}.pdf",
        recipients_notified=request.recipients
    )

@app.post("/schedule", response_model=ScheduledReportResponse)
async def schedule_report(request: ScheduledReportRequest):
    logger.info("Scheduling report", company=request.company_id, type=request.report_type)
    
    return ScheduledReportResponse(
        schedule_id=f"SCH-{datetime.now().strftime('%Y%m%d%H%M')}",
        report_type=request.report_type,
        schedule=request.schedule,
        next_run=datetime.now(),
        active=True
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8361)
