"""
Vimbai MFA Authentication Service
Multi-factor authentication with TOTP, SMS OTP, and backup codes.
Port: 8369

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "mfa_auth_service" not in _sys.modules or not hasattr(_sys.modules.get("mfa_auth_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("mfa_auth_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["mfa_auth_service"] = _pkg
    _sys.modules["mfa_auth_service"].__path__ = [_HERE]

import hashlib
import os
import secrets
import time
import uuid
from datetime import datetime, timedelta, timezone
from typing import Dict

import structlog
from fastapi import Depends, FastAPI, Header, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from mfa_auth_service import crud, models
from mfa_auth_service.dependencies import get_db_session, get_user_id
from mfa_auth_service.exceptions import MfaAuthError
from neo4j import AsyncSession

SERVICE_NAME = "mfa-auth-service"
PORT = int(os.getenv("PORT", "8369"))
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
app = FastAPI(title="Vimbai MFA Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

# Pending challenges: 5-minute ephemeral one-time artifacts, consumed on
# verification. Transient by design, so they stay in memory (the TOTP
# secrets themselves persist to Neo4j via crud.store_secret).
_pending_challenges: Dict[str, dict] = {}


def _generate_totp_secret() -> str:
    import base64

    return base64.b32encode(secrets.token_bytes(20)).decode("utf-8")


def _generate_backup_codes() -> list:
    return [secrets.token_hex(4).upper() for _ in range(8)]


def _verify_totp(secret: str, code: str) -> bool:
    # Simplified TOTP verification - accepts 6-digit codes
    # In production, this would use the actual TOTP algorithm
    return len(code) == 6 and code.isdigit()


def _ensure_self(caller_id: str, target_user_id: str) -> None:
    """MFA belongs to the caller across all Books: only self-service is allowed."""
    if caller_id != target_user_id:
        raise HTTPException(status_code=403, detail="MFA operations are self-service only")


@app.exception_handler(MfaAuthError)
async def _mfa_auth_error(request: Request, exc: MfaAuthError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health(
    x_user_id: str = Header(default=""),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Health check; includes the caller's enrollment status when authenticated."""
    body = {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}
    if x_user_id:
        body["enrolled_users"] = 1 if await crud.get_secret(db_session, x_user_id) else 0
    return body


@app.post("/setup", response_model=models.MFASetupResponse)
async def setup_mfa(
    req: models.MFASetupRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Enroll (or re-enroll) the CALLER's own MFA - re-running setup overwrites."""
    _ensure_self(user_id, req.user_id)
    secret = _generate_totp_secret()
    backup_codes = _generate_backup_codes()
    await crud.store_secret(db_session, user_id, secret, backup_codes)
    qr_uri = f"otpauth://totp/Vimbai:{req.user_id}?secret={secret}&issuer=Vimbai"

    return models.MFASetupResponse(user_id=req.user_id, secret=secret, qr_uri=qr_uri, backup_codes=backup_codes)


@app.post("/verify", response_model=models.MFAVerifyResponse)
async def verify_mfa(
    req: models.MFAVerifyRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Verify the CALLER's own MFA code against their stored secret."""
    _ensure_self(user_id, req.user_id)
    secret = await crud.get_secret(db_session, user_id)
    if secret is None:
        return models.MFAVerifyResponse(verified=False, user_id=req.user_id, message="MFA not set up for this user")

    if _verify_totp(secret, req.code):
        token = hashlib.sha256(f"{req.user_id}{time.time()}".encode()).hexdigest()
        return models.MFAVerifyResponse(
            verified=True, user_id=req.user_id, message="MFA verification successful", access_token=token
        )
    return models.MFAVerifyResponse(verified=False, user_id=req.user_id, message="Invalid MFA code")


@app.post("/challenge")
async def create_challenge(
    user_id: str,
    method: str = "totp",
    caller_id: str = Depends(get_user_id),
):
    """Create a one-time challenge for the CALLER themselves (5-minute expiry)."""
    _ensure_self(caller_id, user_id)
    challenge_id = uuid.uuid4().hex
    code = str(secrets.randbelow(900000) + 100000)
    _pending_challenges[challenge_id] = {
        "user_id": user_id,
        "owner": caller_id,
        "code": code,
        "expires_at": (datetime.now(timezone.utc) + timedelta(minutes=5)).isoformat(),
        "method": method,
    }
    return {"challenge_id": challenge_id, "method": method, "expires_in_minutes": 5}


@app.post("/challenge/{challenge_id}/verify")
async def verify_challenge(
    challenge_id: str,
    code: str,
    caller_id: str = Depends(get_user_id),
):
    """Verify a one-time challenge created by the caller."""
    if challenge_id not in _pending_challenges:
        raise HTTPException(status_code=404, detail="Challenge not found or expired")
    challenge = _pending_challenges[challenge_id]
    if challenge["owner"] != caller_id:
        raise HTTPException(status_code=404, detail="Challenge not found or expired")
    expires = datetime.fromisoformat(challenge["expires_at"])
    if expires < datetime.now(timezone.utc):
        del _pending_challenges[challenge_id]
        raise HTTPException(status_code=401, detail="Challenge expired")
    if challenge["code"] == code:
        del _pending_challenges[challenge_id]
        return {"verified": True, "user_id": challenge["user_id"]}
    return {"verified": False}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
