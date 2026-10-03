"""
Vimbai Capital Reconstruction Service
Handles capital reconstruction schemes (simplification, restructuring,
write-off of excess capital, consolidation, substitution).

Records persist in Neo4j, stamped with the caller (X-User-Id) and the
Book context (X-Book-ID, verified upstream by the API gateway). Child
writes (adjustments, reserve conversions) are authorized against the
caller's own Book-visible reconstruction. Original math, journal
side-call behavior, miss responses, and list shapes are preserved.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import Any, Dict, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "capital_reconstruction_service" not in _sys.modules or not hasattr(
    _sys.modules.get("capital_reconstruction_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "capital_reconstruction_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["capital_reconstruction_service"] = _pkg
    _sys.modules["capital_reconstruction_service"].__path__ = [_HERE]

import httpx
import structlog
from capital_reconstruction_service import crud
from capital_reconstruction_service.database import Neo4jConnector
from capital_reconstruction_service.dependencies import book_id_var, get_db_session, get_user_id
from capital_reconstruction_service.exceptions import CapitalReconstructionServiceError
from capital_reconstruction_service.models import (
    CapitalReconstruction,
    ReconstructionAdjustment,
    ReserveConversion,
)
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "capital-reconstruction-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8062"))
ACCOUNTING_SERVICE_URL = _os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Capital Reconstruction Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(CapitalReconstructionServiceError)
async def _cr_error(request: Request, exc: CapitalReconstructionServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "description": "Capital reconstruction schemes"}


@app.post("/reconstructions/create")
async def create_reconstruction(
    company_id: str,
    reconstruction_type: str,
    description: str,
    scheme_date: datetime,
    previous_share_capital: float,
    new_share_capital: float,
    share_consolidation_ratio: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create capital reconstruction scheme."""
    reconstruction = CapitalReconstruction(
        company_id=company_id,
        reconstruction_type=reconstruction_type,
        description=description,
        scheme_date=scheme_date,
        previous_share_capital=previous_share_capital,
        new_share_capital=new_share_capital,
        share_consolidation_ratio=share_consolidation_ratio,
    )
    reconstruction.capital_reduction_amount = previous_share_capital - new_share_capital
    return await crud.create_reconstruction(db_session, user_id, reconstruction)


@app.post("/reconstructions/{reconstruction_id}/adjustments/add")
async def add_adjustment(
    reconstruction_id: str,
    account_code: str,
    account_name: str,
    previous_balance: float,
    adjustment_type: str,
    adjustment_amount: float,
    description: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add reconstruction adjustment (caller's reconstruction only)."""
    reconstruction = await crud.get_reconstruction(db_session, user_id, reconstruction_id)
    if not reconstruction:
        return {"error": "Reconstruction not found"}

    adjustment = ReconstructionAdjustment(
        reconstruction_id=reconstruction_id,
        account_code=account_code,
        account_name=account_name,
        previous_balance=previous_balance,
        adjustment_type=adjustment_type,
        adjustment_amount=adjustment_amount,
        description=description,
    )
    adjustment.new_balance = previous_balance + adjustment_amount if adjustment_type == "transfer" else 0

    return await crud.create_adjustment(db_session, user_id, adjustment)


@app.post("/reconstructions/{reconstruction_id}/reserve-conversions/add")
async def add_reserve_conversion(
    reconstruction_id: str,
    from_account: str,
    from_account_name: str,
    to_account: str,
    to_account_name: str,
    amount: float,
    reason: str,
    conversion_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add reserve conversion entry (caller's reconstruction only)."""
    reconstruction = await crud.get_reconstruction(db_session, user_id, reconstruction_id)
    if not reconstruction:
        return {"error": "Reconstruction not found"}

    if conversion_date is None:
        conversion_date = datetime.now(timezone.utc)

    conversion = ReserveConversion(
        reconstruction_id=reconstruction_id,
        from_account=from_account,
        from_account_name=from_account_name,
        to_account=to_account,
        to_account_name=to_account_name,
        amount=amount,
        conversion_date=conversion_date,
        reason=reason,
    )

    journal_entry = {
        "date": conversion_date,
        "description": f"Reserve conversion: {from_account_name} to {to_account_name}",
        "entries": [
            {"account_code": from_account, "description": from_account_name, "debit": amount, "credit": 0},
            {"account_code": to_account, "description": to_account_name, "debit": 0, "credit": amount},
        ],
        "reference": f"RCONV-{conversion.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    conversion.journal_entry_id = result.get("id")

    return await crud.create_conversion(db_session, user_id, conversion)


@app.post("/reconstructions/{reconstruction_id}/approve")
async def approve_reconstruction(
    reconstruction_id: str,
    court_approval_date: Optional[datetime] = None,
    shareholders_approval_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Approve and execute reconstruction (caller's reconstruction only)."""
    reconstruction = await crud.get_reconstruction(db_session, user_id, reconstruction_id)
    if not reconstruction:
        return {"error": "Reconstruction not found"}

    if court_approval_date:
        reconstruction.court_approval_date = court_approval_date
    if shareholders_approval_date:
        reconstruction.shareholders_approval_date = shareholders_approval_date

    reconstruction.status = "approved"

    reconstruction_adjustments = await crud.list_adjustments(db_session, user_id, reconstruction_id)
    reconstruction_conversions = await crud.list_conversions(db_session, user_id, reconstruction_id)

    entries = [
        {
            "account_code": "3200",
            "description": "Share Capital",
            "debit": reconstruction.capital_reduction_amount,
            "credit": 0,
        },
    ]

    for adj in reconstruction_adjustments:
        if adj.adjustment_type == "write_off":
            entries.append(
                {
                    "account_code": adj.account_code,
                    "description": adj.account_name,
                    "debit": adj.adjustment_amount,
                    "credit": 0,
                }
            )

    total_credits = reconstruction.capital_reduction_amount + sum(
        adj.adjustment_amount for adj in reconstruction_adjustments if adj.adjustment_type == "write_off"
    )
    entries.append(
        {"account_code": "3300", "description": "Retained Earnings / P&L", "debit": 0, "credit": total_credits}
    )

    journal_entry = {
        "date": reconstruction.scheme_date,
        "description": f"Capital reconstruction: {reconstruction.description}",
        "entries": entries,
        "reference": f"RECONST-{reconstruction.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    reconstruction.journal_entry_id = result.get("id")
    reconstruction.status = "completed"

    await crud.save_reconstruction(db_session, user_id, reconstruction)

    return {
        "reconstruction": reconstruction,
        "adjustments": reconstruction_adjustments,
        "conversions": reconstruction_conversions,
    }


@app.get("/reconstructions")
async def list_reconstructions(
    company_id: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's capital reconstructions."""
    result = await crud.list_reconstructions(db_session, user_id)
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    if status:
        result = [r for r in result if r.status == status]
    return {"reconstructions": result}


@app.get("/reconstructions/{reconstruction_id}")
async def get_reconstruction(
    reconstruction_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get reconstruction details with adjustments (caller-scoped)."""
    reconstruction = await crud.get_reconstruction(db_session, user_id, reconstruction_id)
    if not reconstruction:
        return {"error": "Reconstruction not found"}

    reconstruction_adjustments = await crud.list_adjustments(db_session, user_id, reconstruction_id)
    reconstruction_conversions = await crud.list_conversions(db_session, user_id, reconstruction_id)

    return {
        "reconstruction": reconstruction,
        "adjustments": reconstruction_adjustments,
        "reserve_conversions": reconstruction_conversions,
    }


@app.get("/summary/{company_id}")
async def get_reconstruction_summary(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's capital reconstruction summary for a company."""
    company_reconstructions = [
        r for r in await crud.list_reconstructions(db_session, user_id) if r.company_id == company_id
    ]

    total_reduction = sum(r.capital_reduction_amount for r in company_reconstructions)
    completed = len([r for r in company_reconstructions if r.status == "completed"])

    return {
        "company_id": company_id,
        "total_reconstructions": len(company_reconstructions),
        "completed_reconstructions": completed,
        "total_capital_reduced": total_reduction,
        "reconstructions": company_reconstructions,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
