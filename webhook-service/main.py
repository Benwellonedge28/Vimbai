"""Vimbai Webhook Service - manage outbound webhooks and event notifications. Port: 8364

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "webhook_service" not in _sys.modules or not hasattr(_sys.modules.get("webhook_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("webhook_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["webhook_service"] = _pkg
    _sys.modules["webhook_service"].__path__ = [_HERE]

import hashlib
import hmac
import json
import os
from datetime import datetime, timezone
from typing import Any, Dict

import httpx
import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from webhook_service import crud, models
from webhook_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "webhook-service"
PORT = int(os.getenv("PORT", "8364"))
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
app = FastAPI(title="Vimbai Webhook Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/endpoints")
async def create_endpoint(
    endpoint: models.WebhookEndpointCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    created = await crud.create_endpoint(db_session, user_id, endpoint)
    logger.info("webhook_endpoint_created", company_id=created.company_id, url=created.url)
    return {"id": created.id, "url": created.url, "events": created.events}


@app.get("/endpoints/{company_id}")
async def get_endpoints(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_endpoints(db_session, user_id, company_id)


@app.post("/dispatch/{company_id}")
async def dispatch_webhook(
    company_id: str,
    event_type: str,
    payload: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Dispatch an event to the caller's Book-visible subscribed endpoints.

    Delivery attempts are recorded per endpoint; each outcome persists as a
    Book-stamped delivery record.
    """
    endpoints = await crud.get_active_endpoints_for_event(db_session, user_id, company_id, event_type)
    deliveries = []
    for ep in endpoints:
        delivery = models.WebhookDelivery(
            user_id=user_id,
            book_id=book_id_var.get(),
            endpoint_id=ep.id,
            event_type=event_type,
            payload=payload,
        )
        try:
            body = json.dumps(payload)
            headers = {"Content-Type": "application/json", "X-Event-Type": event_type}
            if ep.secret:
                sig = hmac.new(ep.secret.encode(), body.encode(), hashlib.sha256).hexdigest()
                headers["X-Signature-256"] = sig
            async with httpx.AsyncClient(timeout=10) as client:
                resp = await client.post(ep.url, content=body, headers=headers)
                delivery.status = "delivered" if resp.status_code < 400 else "failed"
                delivery.response_code = resp.status_code
                delivery.attempts = 1
                delivery.last_attempt = datetime.now(timezone.utc)
        except Exception as e:
            delivery.status = "failed"
            delivery.attempts = 1
            delivery.last_attempt = datetime.now(timezone.utc)
            logger.error("webhook_failed", endpoint=ep.url, error=str(e))
        await crud.store_delivery(db_session, user_id, delivery)
        deliveries.append(delivery)
    return {
        "company_id": company_id,
        "event_type": event_type,
        "deliveries": deliveries,
        "total_sent": len(deliveries),
        "successful": sum(1 for d in deliveries if d.status == "delivered"),
    }


@app.get("/deliveries/{endpoint_id}")
async def get_deliveries(
    endpoint_id: str,
    limit: int = 50,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_deliveries(db_session, user_id, endpoint_id, limit)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
