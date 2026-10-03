"""
Vimbai Partnership Agreement Service
Partnership agreement registry.

Agreements persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway). All
lookups are scoped to the caller's own Book-visible records. Original
status codes (201 create / 404 not found) and setattr-based update
semantics are preserved.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "partnership_agreement_service" not in _sys.modules or not hasattr(
    _sys.modules.get("partnership_agreement_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("partnership_agreement_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["partnership_agreement_service"] = _pkg
    _sys.modules["partnership_agreement_service"].__path__ = [_HERE]

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from partnership_agreement_service import crud
from partnership_agreement_service.database import Neo4jConnector
from partnership_agreement_service.dependencies import book_id_var, get_db_session, get_user_id
from partnership_agreement_service.exceptions import PartnershipAgreementServiceError
from partnership_agreement_service.models import Partner, PartnershipAgreement

SERVICE_NAME = "partnership-agreement-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8078"))

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

app = FastAPI(title="Vimbai Partnership Agreement Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(PartnershipAgreementServiceError)
async def _pa_error(request: Request, exc: PartnershipAgreementServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "description": "Partnership agreement registry"}


@app.post("/agreements", response_model=PartnershipAgreement, status_code=status.HTTP_201_CREATED)
async def create_agreement(
    data: PartnershipAgreement,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new partnership agreement."""
    data.id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)
    data.created_at = now
    data.updated_at = now
    data.capital_amount = sum(p.contribution for p in data.partners)
    return await crud.create_agreement(db_session, user_id, data)


@app.get("/agreements/{agreement_id}", response_model=PartnershipAgreement)
async def get_agreement(
    agreement_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get agreement details (caller-scoped)."""
    agg = await crud.get_agreement(db_session, user_id, agreement_id)
    if not agg:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    return agg


@app.get("/agreements", response_model=Dict[str, Any])
async def list_agreements(
    is_active: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's agreements."""
    result = await crud.list_agreements(db_session, user_id)
    if is_active is not None:
        result = [a for a in result if a.is_active == is_active]
    return {"agreements": result, "count": len(result)}


@app.put("/agreements/{agreement_id}", response_model=PartnershipAgreement)
async def update_agreement(
    agreement_id: str,
    data: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update agreement (setattr semantics preserved)."""
    agg = await crud.get_agreement(db_session, user_id, agreement_id)
    if not agg:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    for key, value in data.items():
        if hasattr(agg, key):
            setattr(agg, key, value)
    agg.updated_at = datetime.now(timezone.utc)
    await crud.save_agreement(db_session, user_id, agg)
    return agg


@app.post("/agreements/{agreement_id}/partners/add", response_model=PartnershipAgreement)
async def add_partner(
    agreement_id: str,
    partner: Partner,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a new partner to agreement."""
    agg = await crud.get_agreement(db_session, user_id, agreement_id)
    if not agg:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    agg.partners.append(partner)
    agg.capital_amount += partner.contribution
    agg.updated_at = datetime.now(timezone.utc)
    await crud.save_agreement(db_session, user_id, agg)
    return agg


@app.get("/agreements/{agreement_id}/summary")
async def get_agreement_summary(
    agreement_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get partnership summary (caller-scoped)."""
    agg = await crud.get_agreement(db_session, user_id, agreement_id)
    if not agg:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    return {
        "partnership_name": agg.partnership_name,
        "total_capital": agg.capital_amount,
        "partner_count": len(agg.partners),
        "partners": [
            {"name": p.name, "contribution": p.contribution, "profit_share": p.profit_sharing_ratio}
            for p in agg.partners
        ],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
