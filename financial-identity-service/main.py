"""Vimbai Financial Identity Service - Financial identity verification. Port: 8372"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "financial-identity-service"
PORT = int(os.getenv("PORT", "8372"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Financial Identity Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="financial-identity-service", instrument_app=app)
except ImportError:
    TRACER = None

class VerificationStatus(str, Enum):
    PENDING = "pending"; VERIFIED = "verified"; FAILED = "failed"; EXPIRED = "expired"

class FinancialProfile(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    legal_name: str
    national_id: str = ""
    tax_id: str = ""
    date_of_birth: Optional[datetime] = None
    address: str = ""
    phone: str = ""
    email: str = ""
    employer: str = ""
    annual_income: float = 0
    verification_status: VerificationStatus = VerificationStatus.PENDING
    verified_at: Optional[datetime] = None
    risk_score: int = 0
    kyc_documents: List[str] = []

_profiles: Dict[str, FinancialProfile] = {}

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/profiles", response_model=FinancialProfile)
async def create_profile(profile: FinancialProfile):
    _profiles[profile.id] = profile
    return profile

@app.get("/profiles/{profile_id}")
async def get_profile(profile_id: str):
    if profile_id not in _profiles: raise HTTPException(status_code=404, detail="Profile not found")
    return _profiles[profile_id]

@app.put("/profiles/{profile_id}/verify")
async def verify_profile(profile_id: str, documents: List[str]):
    if profile_id not in _profiles: raise HTTPException(status_code=404, detail="Profile not found")
    p = _profiles[profile_id]
    p.kyc_documents = documents
    if len(documents) >= 2:
        p.verification_status = VerificationStatus.VERIFIED
        p.verified_at = datetime.now(timezone.utc)
        p.risk_score = 20  # low risk
    else:
        p.risk_score = 80  # high risk
    return {"id": profile_id, "status": p.verification_status.value, "risk_score": p.risk_score}

@app.get("/profiles/user/{user_id}")
async def get_by_user(user_id: str):
    for p in _profiles.values():
        if p.user_id == user_id:
            return p
    raise HTTPException(status_code=404, detail="No profile for user")

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
