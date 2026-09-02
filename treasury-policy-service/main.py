"""
Vimbai Treasury Policy Service
Manages treasury policies, limits, and compliance monitoring.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "treasury-policy-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8422"))

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

app = FastAPI(title="Vimbai Treasury Policy Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class TreasuryPolicy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    policy_category: str  # liquidity, funding, investment, fx_risk, interest_rate, counterparty
    version: str = "1.0"
    effective_date: datetime
    review_date: Optional[datetime] = None
    approved_by: str = ""
    status: str = "active"  # draft, active, superseded, retired
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PolicyLimit(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    policy_id: str
    limit_type: str  # exposure, concentration, duration, counterparty
    limit_value: float
    currency: str = "USD"
    warning_threshold: float = 0.8  # 80% of limit
    current_utilization: float = 0.0


class ComplianceCheck(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    policy_id: str
    limit_id: str
    checked_value: float
    limit_value: float
    compliant: bool
    utilization_pct: float
    checked_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    notes: str = ""


policies: List[TreasuryPolicy] = []
limits: List[PolicyLimit] = []
compliance_checks: List[ComplianceCheck] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/policies", response_model=TreasuryPolicy)
async def create_policy(
    name: str,
    description: str,
    policy_category: str,
    effective_date: datetime,
    approved_by: str = "",
    review_date: Optional[datetime] = None,
):
    """Create a treasury policy."""
    valid_cats = ["liquidity", "funding", "investment", "fx_risk", "interest_rate", "counterparty"]
    if policy_category not in valid_cats:
        raise HTTPException(status_code=400, detail=f"Invalid category. Must be one of {valid_cats}")

    policy = TreasuryPolicy(
        name=name,
        description=description,
        policy_category=policy_category,
        effective_date=effective_date,
        approved_by=approved_by,
        review_date=review_date,
    )
    policies.append(policy)
    logger.info("Treasury policy created", policy_id=policy.id, name=name, category=policy_category)
    return policy


@app.get("/policies", response_model=List[TreasuryPolicy])
async def list_policies(category: Optional[str] = None, status: Optional[str] = None):
    """List treasury policies."""
    result = policies
    if category:
        result = [p for p in result if p.policy_category == category]
    if status:
        result = [p for p in result if p.status == status]
    return result


@app.post("/policies/{policy_id}/limits", response_model=PolicyLimit)
async def set_limit(
    policy_id: str,
    limit_type: str,
    limit_value: float,
    currency: str = "USD",
    warning_threshold: float = 0.8,
):
    """Set a limit for a treasury policy."""
    policy = next((p for p in policies if p.id == policy_id), None)
    if not policy:
        raise HTTPException(status_code=404, detail="Policy not found")

    limit = PolicyLimit(
        policy_id=policy_id,
        limit_type=limit_type,
        limit_value=limit_value,
        currency=currency,
        warning_threshold=warning_threshold,
    )
    limits.append(limit)
    logger.info("Policy limit set", limit_id=limit.id, policy_id=policy_id, type=limit_type)
    return limit


@app.get("/policies/{policy_id}/limits", response_model=List[PolicyLimit])
async def list_limits(policy_id: str):
    """List limits for a policy."""
    return [l for l in limits if l.policy_id == policy_id]


@app.post("/limits/{limit_id}/check", response_model=ComplianceCheck)
async def check_compliance(limit_id: str, checked_value: float, notes: str = ""):
    """Check a value against a policy limit."""
    limit = next((l for l in limits if l.id == limit_id), None)
    if not limit:
        raise HTTPException(status_code=404, detail="Limit not found")

    utilization_pct = (checked_value / limit.limit_value * 100) if limit.limit_value > 0 else 0
    compliant = checked_value <= limit.limit_value

    check = ComplianceCheck(
        policy_id=limit.policy_id,
        limit_id=limit_id,
        checked_value=checked_value,
        limit_value=limit.limit_value,
        compliant=compliant,
        utilization_pct=utilization_pct,
        notes=notes,
    )
    compliance_checks.append(check)

    limit.current_utilization = checked_value
    if not compliant:
        logger.warning("Limit breach detected", limit_id=limit_id, value=checked_value, limit=limit.limit_value)

    return check


@app.get("/compliance", response_model=List[ComplianceCheck])
async def list_compliance_checks(policy_id: Optional[str] = None, limit: int = 50):
    """List compliance checks."""
    result = compliance_checks
    if policy_id:
        result = [c for c in result if c.policy_id == policy_id]
    return result[-limit:]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
