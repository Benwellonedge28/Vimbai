"""
Vimbai Treasury Policy Service
Manages treasury policies, limits, and compliance monitoring.
Caller-owned (X-User-Id) and Book-gated (X-Book-ID) stores persist in Neo4j.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import logging
import os as _os
import sys as _sys
from datetime import datetime
from typing import List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "treasury_policy_service" not in _sys.modules or not hasattr(
    _sys.modules.get("treasury_policy_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("treasury_policy_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["treasury_policy_service"] = _pkg
    _sys.modules["treasury_policy_service"].__path__ = [_HERE]

from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from treasury_policy_service import crud
from treasury_policy_service.dependencies import book_id_var, get_db_session, get_user_id
from treasury_policy_service.models import ComplianceCheck, PolicyLimit, TreasuryPolicy

SERVICE_NAME = "treasury-policy-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8263"))

try:
    import structlog

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
except ImportError:  # pragma: no cover
    logging.basicConfig(level=logging.INFO)
    logger = logging.getLogger(SERVICE_NAME)

app = FastAPI(title="Vimbai Treasury Policy Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
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
    saved = await crud.create_policy(db_session, user_id, policy)
    logger.info("Treasury policy created", policy_id=saved.id, name=name, category=policy_category)
    return saved


@app.get("/policies", response_model=List[TreasuryPolicy])
async def list_policies(
    category: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List treasury policies (caller's own Book-visible set)."""
    result = await crud.list_policies(db_session, user_id)
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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Set a limit for a treasury policy (caller's own policies only)."""
    if not await crud.get_policy(db_session, user_id, policy_id):
        raise HTTPException(status_code=404, detail="Policy not found")

    limit = PolicyLimit(
        policy_id=policy_id,
        limit_type=limit_type,
        limit_value=limit_value,
        currency=currency,
        warning_threshold=warning_threshold,
    )
    saved = await crud.create_limit(db_session, user_id, policy_id, limit)
    logger.info("Policy limit set", limit_id=saved.id, policy_id=policy_id, type=limit_type)
    return saved


@app.get("/policies/{policy_id}/limits", response_model=List[PolicyLimit])
async def list_limits(
    policy_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List limits for a policy (caller's own Book-visible set)."""
    return await crud.list_limits(db_session, user_id, policy_id)


@app.post("/limits/{limit_id}/check", response_model=ComplianceCheck)
async def check_compliance(
    limit_id: str,
    checked_value: float,
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Check a value against a policy limit (caller's own limits only)."""
    limit = await crud.get_limit(db_session, user_id, limit_id)
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
    saved = await crud.create_check(db_session, user_id, check)

    # persist the latest utilization on the limit (original semantics)
    await crud.set_limit_utilization(db_session, user_id, limit_id, checked_value)
    if not compliant:
        logger.warning("Limit breach detected", limit_id=limit_id, value=checked_value, limit=limit.limit_value)

    return saved


@app.get("/compliance", response_model=List[ComplianceCheck])
async def list_compliance_checks(
    policy_id: Optional[str] = None,
    limit: int = 50,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List compliance checks (caller's own Book-visible set, most recent `limit` first)."""
    result = await crud.list_checks(db_session, user_id)
    if policy_id:
        result = [c for c in result if c.policy_id == policy_id]
    # original semantics: last N entries in insertion order
    return result[-limit:]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
