"""
Vimbai Report Distribution Service
Manages scheduled report distribution to subscribers via multiple channels.
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

SERVICE_NAME = "report-distribution-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8273"))

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

# Imported both bare (uvicorn main:app) and by path (root integration tests), so
# make sure this directory is importable before pulling in the store module.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import report_distribution_store as store  # noqa: E402

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


# Distribution lists, subscriptions and delivery logs are durable records: they
# live in Neo4j, scoped to the calling user and their Book.
LISTS = ("DistributionList", "OWNS_DISTRIBUTION_LIST")
SUBSCRIPTIONS = ("ReportSubscription", "OWNS_SUBSCRIPTION")
LOGS = ("DistributionLog", "OWNS_DISTRIBUTION_LOG")


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the caller and Book context (set by the gateway) to the store."""
    store.user_id_var.set(request.headers.get("X-User-Id"))
    store.book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


def _list_model(node: Dict[str, Any]) -> DistributionList:
    return DistributionList(
        id=node["id"],
        name=node.get("name", ""),
        description=node.get("description", ""),
        recipients=store.loads(node.get("recipients_json")),
        channels=store.loads(node.get("channels_json")),
        created_at=node.get("created_at"),
    )


def _subscription_model(node: Dict[str, Any]) -> ReportSubscription:
    return ReportSubscription(
        id=node["id"],
        report_type=node.get("report_type", ""),
        distribution_list_id=node.get("distribution_list_id", ""),
        frequency=node.get("frequency", "monthly"),
        format=node.get("format", "pdf"),
        next_run=node.get("next_run"),
        active=bool(node.get("active")),
        created_at=node.get("created_at"),
    )


def _log_model(node: Dict[str, Any]) -> DistributionLog:
    return DistributionLog(
        id=node["id"],
        subscription_id=node.get("subscription_id", ""),
        report_type=node.get("report_type", ""),
        recipients=store.loads(node.get("recipients_json")),
        status=node.get("status", "sent"),
        channel=node.get("channel", "email"),
        sent_at=node.get("sent_at"),
        error_message=node.get("error_message", ""),
    )


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
    node = await store.create_node(
        LISTS[0],
        LISTS[1],
        {
            "id": dist_list.id,
            "name": dist_list.name,
            "description": dist_list.description,
            "recipients_json": store.dumps(dist_list.recipients),
            "channels_json": store.dumps(dist_list.channels),
            "created_at": dist_list.created_at.isoformat(),
        },
    )
    logger.info("Distribution list created", list_id=dist_list.id, name=name)
    return _list_model(node)


@app.get("/lists", response_model=List[DistributionList])
async def list_lists():
    """List all distribution lists."""
    nodes = await store.list_nodes(LISTS[0], LISTS[1])
    return [_list_model(node) for node in nodes]


@app.post("/subscriptions", response_model=ReportSubscription)
async def create_subscription(
    report_type: str, distribution_list_id: str, frequency: str = "monthly", fmt: str = "pdf"
):
    """Create a report subscription."""
    dist_list = await store.get_node(LISTS[0], LISTS[1], distribution_list_id)
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
    node = await store.create_node(
        SUBSCRIPTIONS[0],
        SUBSCRIPTIONS[1],
        {
            "id": sub.id,
            "report_type": sub.report_type,
            "distribution_list_id": sub.distribution_list_id,
            "frequency": sub.frequency,
            "format": sub.format,
            "next_run": sub.next_run.isoformat(),
            "active": sub.active,
            "created_at": sub.created_at.isoformat(),
        },
    )
    logger.info("Subscription created", sub_id=sub.id, report_type=report_type, frequency=frequency)
    return _subscription_model(node)


@app.get("/subscriptions", response_model=List[ReportSubscription])
async def list_subscriptions(active: Optional[bool] = None):
    """List report subscriptions."""
    nodes = await store.list_nodes(SUBSCRIPTIONS[0], SUBSCRIPTIONS[1])
    subs = [_subscription_model(node) for node in nodes]
    if active is not None:
        return [s for s in subs if s.active == active]
    return subs


@app.post("/subscriptions/{sub_id}/distribute", response_model=DistributionLog)
async def distribute_report(sub_id: str):
    """Distribute a report to its subscription's distribution list."""
    sub_node = await store.get_node(SUBSCRIPTIONS[0], SUBSCRIPTIONS[1], sub_id)
    if not sub_node:
        raise HTTPException(status_code=404, detail="Subscription not found")
    sub = _subscription_model(sub_node)
    if not sub.active:
        raise HTTPException(status_code=400, detail="Subscription is inactive")

    dist_list_node = await store.get_node(LISTS[0], LISTS[1], sub.distribution_list_id)
    if not dist_list_node:
        raise HTTPException(status_code=404, detail="Distribution list not found")
    dist_list = _list_model(dist_list_node)

    log = DistributionLog(
        subscription_id=sub_id,
        report_type=sub.report_type,
        recipients=dist_list.recipients,
        channel=dist_list.channels[0] if dist_list.channels else "email",
        status="sent",
    )
    node = await store.create_node(
        LOGS[0],
        LOGS[1],
        {
            "id": log.id,
            "subscription_id": log.subscription_id,
            "report_type": log.report_type,
            "recipients_json": store.dumps(log.recipients),
            "status": log.status,
            "channel": log.channel,
            "sent_at": log.sent_at.isoformat(),
            "error_message": log.error_message,
        },
    )
    logger.info("Report distributed", sub_id=sub_id, report_type=sub.report_type, recipients=len(dist_list.recipients))
    return _log_model(node)


@app.get("/logs", response_model=List[DistributionLog])
async def list_logs(limit: int = 50, subscription_id: Optional[str] = None):
    """List distribution logs."""
    nodes = await store.list_nodes(LOGS[0], LOGS[1], {"subscription_id": subscription_id})
    logs = [_log_model(node) for node in nodes]
    return logs[-limit:]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
