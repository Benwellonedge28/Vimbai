"""
Vimbai Zero Trust Data Service
Implements zero-trust access control policies and access evaluation.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "zero-trust-data-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8411"))

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

app = FastAPI(title="Vimbai Zero Trust Data Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class AccessPolicy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    resource: str
    required_roles: List[str] = []
    required_clearance: str = "standard"  # public, internal, confidential, restricted
    mfa_required: bool = True
    ip_whitelist: List[str] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class AccessAttempt(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    resource: str
    policy_id: str
    user_roles: List[str] = []
    user_clearance: str = "standard"
    mfa_verified: bool = False
    source_ip: str = ""
    granted: bool = False
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    reason: str = ""


class EvaluateRequest(BaseModel):
    user_id: str
    resource: str
    user_roles: List[str] = []
    user_clearance: str = "standard"
    mfa_verified: bool = False
    source_ip: str = ""


policies: List[AccessPolicy] = []
attempts: List[AccessAttempt] = []

CLEARANCE_LEVELS = {"public": 0, "internal": 1, "confidential": 2, "restricted": 3}


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/policies", response_model=AccessPolicy)
async def create_policy(policy: AccessPolicy):
    """Create an access control policy."""
    policies.append(policy)
    logger.info("Access policy created", policy_id=policy.id, resource=policy.resource)
    return policy


@app.get("/policies", response_model=List[AccessPolicy])
async def list_policies():
    """List all access policies."""
    return policies


@app.put("/policies/{policy_id}", response_model=AccessPolicy)
async def update_policy(policy_id: str, policy: AccessPolicy):
    """Update an access policy."""
    existing = next((p for p in policies if p.id == policy_id), None)
    if not existing:
        raise HTTPException(status_code=404, detail="Policy not found")

    policy.id = policy_id
    idx = policies.index(existing)
    policies[idx] = policy
    return policy


@app.delete("/policies/{policy_id}")
async def delete_policy(policy_id: str):
    """Delete an access policy."""
    global policies
    existing = next((p for p in policies if p.id == policy_id), None)
    if not existing:
        raise HTTPException(status_code=404, detail="Policy not found")

    policies = [p for p in policies if p.id != policy_id]
    return {"deleted": True, "policy_id": policy_id}


@app.post("/evaluate", response_model=AccessAttempt)
async def evaluate_access(request: EvaluateRequest):
    """Evaluate access request against zero-trust policies."""
    policy = next((p for p in policies if p.resource == request.resource), None)

    if not policy:
        attempt = AccessAttempt(
            user_id=request.user_id,
            resource=request.resource,
            policy_id="",
            user_roles=request.user_roles,
            user_clearance=request.user_clearance,
            mfa_verified=request.mfa_verified,
            source_ip=request.source_ip,
            granted=False,
            reason="No policy found for resource",
        )
        attempts.append(attempt)
        return attempt

    # Check roles
    has_role = any(role in request.user_roles for role in policy.required_roles) if policy.required_roles else True

    # Check clearance
    user_level = CLEARANCE_LEVELS.get(request.user_clearance, 0)
    required_level = CLEARANCE_LEVELS.get(policy.required_clearance, 0)
    has_clearance = user_level >= required_level

    # Check MFA
    mfa_ok = request.mfa_verified if policy.mfa_required else True

    # Check IP whitelist
    ip_ok = True
    if policy.ip_whitelist and request.source_ip not in policy.ip_whitelist:
        ip_ok = False

    granted = has_role and has_clearance and mfa_ok and ip_ok
    reasons = []
    if not has_role:
        reasons.append("Missing required role")
    if not has_clearance:
        reasons.append("Insufficient clearance")
    if not mfa_ok:
        reasons.append("MFA required but not verified")
    if not ip_ok:
        reasons.append("IP not in whitelist")

    attempt = AccessAttempt(
        user_id=request.user_id,
        resource=request.resource,
        policy_id=policy.id,
        user_roles=request.user_roles,
        user_clearance=request.user_clearance,
        mfa_verified=request.mfa_verified,
        source_ip=request.source_ip,
        granted=granted,
        reason="; ".join(reasons) if reasons else "Access granted",
    )
    attempts.append(attempt)
    logger.info("Access evaluated", user=request.user_id, resource=request.resource, granted=granted)
    return attempt


@app.get("/attempts", response_model=List[AccessAttempt])
async def list_attempts(limit: int = 50):
    """List recent access attempts."""
    return attempts[-limit:]


@app.get("/attempts/user/{user_id}", response_model=List[AccessAttempt])
async def user_attempts(user_id: str, limit: int = 50):
    """List access attempts for a specific user."""
    user_atmpts = [a for a in attempts if a.user_id == user_id]
    return user_atmpts[-limit:]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
