"""
Vimbai Privacy Admin Dashboard Service
Handles data subject access requests (DSAR), consent management, and privacy compliance.
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

SERVICE_NAME = "privacy-admin-dashboard-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8003"))

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

import privacy_admin_store as store  # noqa: E402

app = FastAPI(title="Vimbai Privacy Admin Dashboard", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class PrivacyRequest(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    request_type: str  # access, deletion, correction, portability
    subject_name: str
    subject_email: str
    status: str = "pending"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    resolved_at: Optional[datetime] = None
    notes: str = ""


class ConsentRecord(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    subject_email: str
    data_type: str  # marketing, analytics, third_party
    consent_given: bool
    granted_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    revoked_at: Optional[datetime] = None


class UpdatePrivacyRequest(BaseModel):
    status: str
    notes: str = ""


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the caller and Book context (set by the gateway) to the store."""
    store.user_id_var.set(request.headers.get("X-User-Id"))
    store.book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


# Privacy requests and consent records are durable, auditable records: Neo4j,
# caller-owned (:OWNS_* edges), Book-gated.
REQUESTS = ("PrivacyRequest", "OWNS_PRIVACY_REQUEST")
CONSENTS = ("ConsentRecord", "OWNS_CONSENT")


def _request_model(node: Dict[str, Any]) -> PrivacyRequest:
    return PrivacyRequest(
        id=node["id"],
        request_type=node.get("request_type", ""),
        subject_name=node.get("subject_name", ""),
        subject_email=node.get("subject_email", ""),
        status=node.get("status", "pending"),
        created_at=node.get("created_at"),
        resolved_at=node.get("resolved_at"),
        notes=node.get("notes", ""),
    )


def _consent_model(node: Dict[str, Any]) -> ConsentRecord:
    return ConsentRecord(
        id=node["id"],
        subject_email=node.get("subject_email", ""),
        data_type=node.get("data_type", ""),
        consent_given=bool(node.get("consent_given")),
        granted_at=node.get("granted_at"),
        revoked_at=node.get("revoked_at"),
    )


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/requests", response_model=PrivacyRequest)
async def create_request(request_type: str, subject_name: str, subject_email: str, notes: str = ""):
    """Create a data subject access request."""
    valid_types = ["access", "deletion", "correction", "portability"]
    if request_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid request type. Must be one of {valid_types}")

    req = PrivacyRequest(
        request_type=request_type,
        subject_name=subject_name,
        subject_email=subject_email,
        notes=notes,
    )
    node = await store.create_node(
        REQUESTS[0],
        REQUESTS[1],
        {
            "id": req.id,
            "request_type": req.request_type,
            "subject_name": req.subject_name,
            "subject_email": req.subject_email,
            "status": req.status,
            "created_at": req.created_at.isoformat(),
            "resolved_at": None,
            "notes": req.notes,
        },
    )
    logger.info("Privacy request created", request_id=req.id, type=request_type)
    return _request_model(node)


@app.get("/requests", response_model=List[PrivacyRequest])
async def list_requests(status: Optional[str] = None):
    """List all privacy requests, optionally filtered by status."""
    nodes = await store.list_nodes(REQUESTS[0], REQUESTS[1], {"status": status})
    return [_request_model(node) for node in nodes]


@app.put("/requests/{request_id}", response_model=PrivacyRequest)
async def update_request(request_id: str, update: UpdatePrivacyRequest):
    """Update a privacy request status."""
    node = await store.get_node(REQUESTS[0], REQUESTS[1], request_id)
    if not node:
        raise HTTPException(status_code=404, detail="Privacy request not found")

    patch = {"status": update.status, "notes": update.notes}
    if update.status in ("resolved", "rejected", "completed"):
        patch["resolved_at"] = datetime.now(timezone.utc).isoformat()
    updated = await store.update_node(REQUESTS[0], REQUESTS[1], request_id, patch)
    logger.info("Privacy request updated", request_id=request_id, status=update.status)
    return _request_model(updated)


@app.delete("/requests/{request_id}")
async def delete_request(request_id: str):
    """Delete a privacy request."""
    node = await store.get_node(REQUESTS[0], REQUESTS[1], request_id)
    if not node:
        raise HTTPException(status_code=404, detail="Privacy request not found")

    await store.delete_node(REQUESTS[0], REQUESTS[1], request_id)
    return {"deleted": True, "request_id": request_id}


@app.post("/consent", response_model=ConsentRecord)
async def grant_consent(subject_email: str, data_type: str, consent_given: bool = True):
    """Grant or update consent for data processing."""
    record = ConsentRecord(
        subject_email=subject_email,
        data_type=data_type,
        consent_given=consent_given,
    )
    node = await store.create_node(
        CONSENTS[0],
        CONSENTS[1],
        {
            "id": record.id,
            "subject_email": record.subject_email,
            "data_type": record.data_type,
            "consent_given": record.consent_given,
            "granted_at": record.granted_at.isoformat(),
            "revoked_at": None,
        },
    )
    logger.info("Consent recorded", consent_id=record.id, email=subject_email, consent=consent_given)
    return _consent_model(node)


@app.delete("/consent/{consent_id}")
async def revoke_consent(consent_id: str):
    """Revoke a consent record."""
    node = await store.get_node(CONSENTS[0], CONSENTS[1], consent_id)
    if not node:
        raise HTTPException(status_code=404, detail="Consent record not found")

    await store.update_node(
        CONSENTS[0],
        CONSENTS[1],
        consent_id,
        {"revoked_at": datetime.now(timezone.utc).isoformat(), "consent_given": False},
    )
    return {"consent_id": consent_id, "revoked": True}


@app.get("/consent/{email}", response_model=List[ConsentRecord])
async def check_consent(email: str):
    """Check consent records for a given email."""
    nodes = await store.list_nodes(CONSENTS[0], CONSENTS[1])
    records = [_consent_model(node) for node in nodes]
    return [r for r in records if r.subject_email == email and r.revoked_at is None]


@app.get("/dashboard")
async def dashboard():
    """Privacy compliance dashboard summary."""
    requests_ = [_request_model(n) for n in await store.list_nodes(REQUESTS[0], REQUESTS[1])]
    consents = [_consent_model(n) for n in await store.list_nodes(CONSENTS[0], CONSENTS[1])]
    return {
        "total_requests": len(requests_),
        "pending_requests": len([r for r in requests_ if r.status == "pending"]),
        "resolved_requests": len([r for r in requests_ if r.status in ("resolved", "completed")]),
        "active_consents": len([r for r in consents if r.consent_given and r.revoked_at is None]),
        "revoked_consents": len([r for r in consents if r.revoked_at is not None]),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
