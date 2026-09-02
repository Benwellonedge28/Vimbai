"""
Vimbai Report Distribution Service
Manages scheduled report distribution to subscribers via multiple channels.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "report-distribution-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8416"))

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

app = FastAPI(title="Vimbai Report Distribution Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class DistributionList(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    recipients: List[str] = []  # email addresses
    channels: List[str] = ["email"]  # email, webhook, sms
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ReportSubscription(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    report_type: str
    distribution_list_id: str
    frequency: str = "monthly"  # daily, weekly, monthly, quarterly
    format: str = "pdf"  # pdf, xlsx, csv
    next_run: Optional[datetime] = None
    active: bool = True
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DistributionLog(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    subscription_id: str
    report_type: str
    recipients: List[str] = []
    status: str = "sent"  # sent, failed, pending
    channel: str = "email"
    sent_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    error_message: str = ""


distribution_lists: List[DistributionList] = []
subscriptions: List[ReportSubscription] = []
distribution_logs: List[DistributionLog] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/lists", response_model=DistributionList)
async def create_list(name: str, description: str = "", recipients: List[str] = [], channels: List[str] = ["email"]):
    """Create a distribution list."""
    dist_list = DistributionList(
        name=name,
        description=description,
        recipients=recipients,
        channels=channels,
    )
    distribution_lists.append(dist_list)
    logger.info("Distribution list created", list_id=dist_list.id, name=name)
    return dist_list


@app.get("/lists", response_model=List[DistributionList])
async def list_lists():
    """List all distribution lists."""
    return distribution_lists


@app.post("/subscriptions", response_model=ReportSubscription)
async def create_subscription(
    report_type: str, distribution_list_id: str, frequency: str = "monthly", fmt: str = "pdf"
):
    """Create a report subscription."""
    dist_list = next((dl for dl in distribution_lists if dl.id == distribution_list_id), None)
    if not dist_list:
        raise HTTPException(status_code=404, detail="Distribution list not found")

    valid_freqs = ["daily", "weekly", "monthly", "quarterly"]
    if frequency not in valid_freqs:
        raise HTTPException(status_code=400, detail=f"Invalid frequency. Must be one of {valid_freqs}")

    sub = ReportSubscription(
        report_type=report_type,
        distribution_list_id=distribution_list_id,
        frequency=frequency,
        format=fmt,
        next_run=datetime.now(timezone.utc),
    )
    subscriptions.append(sub)
    logger.info("Subscription created", sub_id=sub.id, report_type=report_type, frequency=frequency)
    return sub


@app.get("/subscriptions", response_model=List[ReportSubscription])
async def list_subscriptions(active: Optional[bool] = None):
    """List report subscriptions."""
    if active is not None:
        return [s for s in subscriptions if s.active == active]
    return subscriptions


@app.post("/subscriptions/{sub_id}/distribute", response_model=DistributionLog)
async def distribute_report(sub_id: str):
    """Distribute a report to its subscription's distribution list."""
    sub = next((s for s in subscriptions if s.id == sub_id), None)
    if not sub:
        raise HTTPException(status_code=404, detail="Subscription not found")
    if not sub.active:
        raise HTTPException(status_code=400, detail="Subscription is inactive")

    dist_list = next((dl for dl in distribution_lists if dl.id == sub.distribution_list_id), None)
    if not dist_list:
        raise HTTPException(status_code=404, detail="Distribution list not found")

    log = DistributionLog(
        subscription_id=sub_id,
        report_type=sub.report_type,
        recipients=dist_list.recipients,
        channel=dist_list.channels[0] if dist_list.channels else "email",
        status="sent",
    )
    distribution_logs.append(log)
    logger.info("Report distributed", sub_id=sub_id, report_type=sub.report_type, recipients=len(dist_list.recipients))
    return log


@app.get("/logs", response_model=List[DistributionLog])
async def list_logs(limit: int = 50, subscription_id: Optional[str] = None):
    """List distribution logs."""
    result = distribution_logs
    if subscription_id:
        result = [l for l in result if l.subscription_id == subscription_id]
    return result[-limit:]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
