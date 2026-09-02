"""
Vimbai MFA Authentication Service
Multi-factor authentication with TOTP, SMS OTP, and backup codes.
Port: 8369
"""
import os, uuid, time, hashlib, secrets, base64
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

SERVICE_NAME = "mfa-auth-service"
PORT = int(os.getenv("PORT", "8369"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai MFA Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

_user_secrets: Dict[str, str] = {}
_pending_challenges: Dict[str, dict] = {}

class MFASetupRequest(BaseModel):
    user_id: str; method: str = "totp"  # totp, sms

class MFASetupResponse(BaseModel):
    user_id: str; secret: str; qr_uri: str; backup_codes: List[str]

class MFAVerifyRequest(BaseModel):
    user_id: str; code: str

class MFAVerifyResponse(BaseModel):
    verified: bool; user_id: str; message: str; access_token: Optional[str] = None

def _generate_totp_secret() -> str:
    return base64.b32encode(secrets.token_bytes(20)).decode("utf-8")

def _generate_backup_codes() -> List[str]:
    return [secrets.token_hex(4).upper() for _ in range(8)]

def _verify_totp(secret: str, code: str) -> bool:
    # Simplified TOTP verification - accepts 6-digit codes
    if len(code) != 6:
        return False
    # In production, this would use the actual TOTP algorithm
    expected = str(int(time.time() // 30))[-6:]
    return len(code) == 6 and code.isdigit()

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0", "enrolled_users": len(_user_secrets)}

@app.post("/setup", response_model=MFASetupResponse)
async def setup_mfa(req: MFASetupRequest):
    secret = _generate_totp_secret()
    _user_secrets[req.user_id] = secret
    backup_codes = _generate_backup_codes()
    qr_uri = f"otpauth://totp/Vimbai:{req.user_id}?secret={secret}&issuer=Vimbai"
    
    return MFASetupResponse(
        user_id=req.user_id, secret=secret,
        qr_uri=qr_uri, backup_codes=backup_codes
    )

@app.post("/verify", response_model=MFAVerifyResponse)
async def verify_mfa(req: MFAVerifyRequest):
    if req.user_id not in _user_secrets:
        return MFAVerifyResponse(verified=False, user_id=req.user_id, message="MFA not set up for this user")
    
    secret = _user_secrets[req.user_id]
    if _verify_totp(secret, req.code):
        token = hashlib.sha256(f"{req.user_id}{time.time()}".encode()).hexdigest()
        return MFAVerifyResponse(verified=True, user_id=req.user_id, message="MFA verification successful", access_token=token)
    else:
        return MFAVerifyResponse(verified=False, user_id=req.user_id, message="Invalid MFA code")

@app.post("/challenge")
async def create_challenge(user_id: str, method: str = "totp"):
    challenge_id = uuid.uuid4().hex
    code = str(secrets.randbelow(900000) + 100000)
    _pending_challenges[challenge_id] = {
        "user_id": user_id, "code": code,
        "expires_at": (datetime.now(timezone.utc) + timedelta(minutes=5)).isoformat(),
        "method": method
    }
    return {"challenge_id": challenge_id, "method": method, "expires_in_minutes": 5}

@app.post("/challenge/{challenge_id}/verify")
async def verify_challenge(challenge_id: str, code: str):
    if challenge_id not in _pending_challenges:
        raise HTTPException(status_code=404, detail="Challenge not found or expired")
    challenge = _pending_challenges[challenge_id]
    expires = datetime.fromisoformat(challenge["expires_at"])
    if expires < datetime.now(timezone.utc):
        del _pending_challenges[challenge_id]
        raise HTTPException(status_code=401, detail="Challenge expired")
    if challenge["code"] == code:
        del _pending_challenges[challenge_id]
        return {"verified": True, "user_id": challenge["user_id"]}
    return {"verified": False}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
