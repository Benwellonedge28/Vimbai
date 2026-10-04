"""Vimbai Zero Trust Data Service. Port: 8002.

Zero-trust access control policies and access evaluation. The two
module-level lists (policies, attempts) were process-global and shared
across ALL callers; they now persist to Neo4j as caller-owned,
Book-scoped records (X-User-Id / X-Book-ID). AccessAttempt.user_id is
the access subject and is stored as `subject_user_id` to stay distinct
from the caller-ownership stamp.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "zero_trust_data_service" not in _sys.modules or not hasattr(
    _sys.modules.get("zero_trust_data_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "zero_trust_data_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["zero_trust_data_service"] = _pkg
    _sys.modules["zero_trust_data_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime, timezone
from typing import List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from pydantic import BaseModel, Field

from zero_trust_data_service import crud
from zero_trust_data_service.dependencies import book_id_var, get_db_session, get_user_id
from zero_trust_data_service.exceptions import ZeroTrustDataError
from zero_trust_data_service.models import AccessAttempt, AccessPolicy, EvaluateRequest

SERVICE_NAME = "zero-trust-data-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8002"))

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(ZeroTrustDataError)
async def _zero_trust_error(request: Request, exc: ZeroTrustDataError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400), content={"detail": str(exc), "error": exc.__class__.__name__}
    )


try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


CLEARANCE_LEVELS = {"public": 0, "internal": 1, "confidential": 2, "restricted": 3}


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/policies", response_model=AccessPolicy)
async def create_policy(
    policy: AccessPolicy,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create an access control policy (caller-owned)."""
    await crud.create(db_session, caller_id, policy)
    logger.info("Access policy created", policy_id=policy.id, resource=policy.resource)
    return policy


@app.get("/policies", response_model=List[AccessPolicy])
async def list_policies(
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's access policies."""
    return await crud.list_all(db_session, caller_id, AccessPolicy)


@app.put("/policies/{policy_id}", response_model=AccessPolicy)
async def update_policy(
    policy_id: str,
    policy: AccessPolicy,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update an access policy (full replace, caller-owned)."""
    existing = await crud.find(db_session, caller_id, AccessPolicy, policy_id)
    if not existing:
        raise HTTPException(status_code=404, detail="Policy not found")

    policy.id = policy_id
    await crud.delete_where(db_session, caller_id, AccessPolicy, {"id": policy_id})
    await crud.create(db_session, caller_id, policy)
    return policy


@app.delete("/policies/{policy_id}")
async def delete_policy(
    policy_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete an access policy (caller-scoped 404 like the original)."""
    removed = await crud.delete_where(db_session, caller_id, AccessPolicy, {"id": policy_id})
    if not removed:
        raise HTTPException(status_code=404, detail="Policy not found")
    return {"deleted": True, "policy_id": policy_id}


@app.post("/evaluate", response_model=AccessAttempt)
async def evaluate_access(
    request: EvaluateRequest,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Evaluate an access request against the caller's zero-trust policies."""
    policy = next(
        (p for p in await crud.list_all(db_session, caller_id, AccessPolicy) if p.resource == request.resource),
        None,
    )

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
        await crud.create(db_session, caller_id, attempt)
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
    await crud.create(db_session, caller_id, attempt)
    logger.info("Access evaluated", user=request.user_id, resource=request.resource, granted=granted)
    return attempt


@app.get("/attempts", response_model=List[AccessAttempt])
async def list_attempts(
    limit: int = 50,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's recent access attempts."""
    attempts = await crud.list_all(db_session, caller_id, AccessAttempt)
    attempts.sort(key=lambda a: a.timestamp)
    return attempts[-limit:]


@app.get("/attempts/user/{user_id}", response_model=List[AccessAttempt])
async def user_attempts(
    user_id: str,
    limit: int = 50,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's access attempts for a specific subject user."""
    user_atmpts = [a for a in await crud.list_all(db_session, caller_id, AccessAttempt) if a.user_id == user_id]
    user_atmpts.sort(key=lambda a: a.timestamp)
    return user_atmpts[-limit:]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
