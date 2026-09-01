"""Vimbai Webhook Service - Manage outbound webhooks and event notifications. Port: 8364"""
import os, uuid, hashlib, hmac, json
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from collections import defaultdict
import httpx, structlog
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "webhook-service"
PORT = int(os.getenv("PORT", "8364"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Webhook Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="webhook-service", instrument_app=app)
except ImportError:
    TRACER = None

class WebhookEndpoint(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    url: str
    secret: str = ""
    events: List[str] = []  # e.g., ["invoice.created", "payment.received"]
    active: bool = True
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

class WebhookDelivery(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    endpoint_id: str
    event_type: str
    payload: Dict[str, Any]
    status: str = "pending"  # pending, delivered, failed
    attempts: int = 0
    response_code: int = 0
    last_attempt: Optional[datetime] = None

_endpoints: Dict[str, List[WebhookEndpoint]] = defaultdict(list)
_deliveries: Dict[str, List[WebhookDelivery]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/endpoints")
async def create_endpoint(endpoint: WebhookEndpoint):
    _endpoints[endpoint.company_id].append(endpoint)
    return {"id": endpoint.id, "url": endpoint.url, "events": endpoint.events}

@app.get("/endpoints/{company_id}")
async def get_endpoints(company_id: str):
    return {"company_id": company_id, "endpoints": _endpoints.get(company_id, []), "total": len(_endpoints.get(company_id, []))}

@app.post("/dispatch/{company_id}")
async def dispatch_webhook(company_id: str, event_type: str, payload: Dict[str, Any]):
    endpoints = [e for e in _endpoints.get(company_id, []) if e.active and (not e.events or event_type in e.events)]
    deliveries = []
    for ep in endpoints:
        delivery = WebhookDelivery(endpoint_id=ep.id, event_type=event_type, payload=payload)
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
        deliveries.append(delivery)
        _deliveries[ep.id].append(delivery)
    return {"company_id": company_id, "event_type": event_type, "deliveries": deliveries, "total_sent": len(deliveries), "successful": sum(1 for d in deliveries if d.status == "delivered")}

@app.get("/deliveries/{endpoint_id}")
async def get_deliveries(endpoint_id: str, limit: int = 50):
    return {"endpoint_id": endpoint_id, "deliveries": _deliveries.get(endpoint_id, [])[-limit:], "total": len(_deliveries.get(endpoint_id, []))}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
