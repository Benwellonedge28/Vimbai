"""
Vimbai Privacy Admin Dashboard Service
Handles data subject access requests (DSAR), consent management, and privacy compliance.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
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


privacy_requests: List[PrivacyRequest] = []
consent_records: List[ConsentRecord] = []


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
    privacy_requests.append(req)
    logger.info("Privacy request created", request_id=req.id, type=request_type)
    return req


@app.get("/requests", response_model=List[PrivacyRequest])
async def list_requests(status: Optional[str] = None):
    """List all privacy requests, optionally filtered by status."""
    if status:
        return [r for r in privacy_requests if r.status == status]
    return privacy_requests


@app.put("/requests/{request_id}", response_model=PrivacyRequest)
async def update_request(request_id: str, update: UpdatePrivacyRequest):
    """Update a privacy request status."""
    req = next((r for r in privacy_requests if r.id == request_id), None)
    if not req:
        raise HTTPException(status_code=404, detail="Privacy request not found")

    req.status = update.status
    req.notes = update.notes
    if update.status in ("resolved", "rejected", "completed"):
        req.resolved_at = datetime.now(timezone.utc)
    logger.info("Privacy request updated", request_id=request_id, status=update.status)
    return req


@app.delete("/requests/{request_id}")
async def delete_request(request_id: str):
    """Delete a privacy request."""
    global privacy_requests
    req = next((r for r in privacy_requests if r.id == request_id), None)
    if not req:
        raise HTTPException(status_code=404, detail="Privacy request not found")

    privacy_requests = [r for r in privacy_requests if r.id != request_id]
    return {"deleted": True, "request_id": request_id}


@app.post("/consent", response_model=ConsentRecord)
async def grant_consent(subject_email: str, data_type: str, consent_given: bool = True):
    """Grant or update consent for data processing."""
    record = ConsentRecord(
        subject_email=subject_email,
        data_type=data_type,
        consent_given=consent_given,
    )
    consent_records.append(record)
    logger.info("Consent recorded", consent_id=record.id, email=subject_email, consent=consent_given)
    return record


@app.delete("/consent/{consent_id}")
async def revoke_consent(consent_id: str):
    """Revoke a consent record."""
    record = next((r for r in consent_records if r.id == consent_id), None)
    if not record:
        raise HTTPException(status_code=404, detail="Consent record not found")

    record.revoked_at = datetime.now(timezone.utc)
    record.consent_given = False
    return {"consent_id": consent_id, "revoked": True}


@app.get("/consent/{email}", response_model=List[ConsentRecord])
async def check_consent(email: str):
    """Check consent records for a given email."""
    return [r for r in consent_records if r.subject_email == email and r.revoked_at is None]


@app.get("/dashboard")
async def dashboard():
    """Privacy compliance dashboard summary."""
    return {
        "total_requests": len(privacy_requests),
        "pending_requests": len([r for r in privacy_requests if r.status == "pending"]),
        "resolved_requests": len([r for r in privacy_requests if r.status in ("resolved", "completed")]),
        "active_consents": len([r for r in consent_records if r.consent_given and r.revoked_at is None]),
        "revoked_consents": len([r for r in consent_records if r.revoked_at is not None]),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
