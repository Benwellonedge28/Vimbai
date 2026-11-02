"""
Vimbai Lease Termination Service
Lease exit workflow with settlements.

Terminations and settlements live in Neo4j, scoped to the caller and the Book. The
file may be imported bare (bracket mounts, uvicorn main:app), so it bootstraps its
own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "lease_termination_service" not in _sys.modules or not hasattr(
    _sys.modules.get("lease_termination_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("lease_termination_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["lease_termination_service"] = _pkg
    _sys.modules["lease_termination_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import List, Optional

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from lease_termination_service import crud
from lease_termination_service.dependencies import book_id_var, get_db_session, get_user_id
from lease_termination_service.exceptions import ConflictError, NotFoundError
from lease_termination_service.models import LeaseTermination, TerminationSettlement
from neo4j import AsyncSession

SERVICE_NAME = "lease-termination-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8224"))

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

app = FastAPI(title="Vimbai Lease Termination Service", version=SERVICE_VERSION, docs_url="/docs")
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


@app.post("/terminations", response_model=LeaseTermination)
async def create_termination(
    lease_id: str,
    termination_date: datetime,
    original_end_date: datetime,
    remaining_payments: int,
    remaining_payment_amount: float,
    early_termination_penalty: float = 0.0,
    reason: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Initiate a lease termination."""
    termination = await crud.create_termination(
        db_session,
        user_id,
        lease_id=lease_id,
        termination_date=termination_date,
        original_end_date=original_end_date,
        remaining_payments=remaining_payments,
        remaining_payment_amount=remaining_payment_amount,
        early_termination_penalty=early_termination_penalty,
        reason=reason,
    )
    logger.info("Lease termination initiated", termination_id=termination.id, lease_id=lease_id)
    return termination


@app.get("/terminations", response_model=List[LeaseTermination])
async def list_terminations(
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List lease terminations."""
    return await crud.list_terminations(db_session, user_id, status)


@app.get("/terminations/{termination_id}", response_model=LeaseTermination)
async def get_termination(
    termination_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific termination."""
    try:
        return await crud.get_termination(db_session, user_id, termination_id)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Termination not found")


@app.post("/terminations/{termination_id}/settlement", response_model=TerminationSettlement)
async def calculate_settlement(
    termination_id: str,
    asset_return_value: float = 0.0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate and record the settlement for a lease termination."""
    try:
        settlement = await crud.calculate_settlement(db_session, user_id, termination_id, asset_return_value)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Termination not found")
    logger.info(
        "Settlement calculated",
        termination_id=termination_id,
        net_settlement=settlement.net_settlement,
    )
    return settlement


@app.put("/terminations/{termination_id}/approve")
async def approve_termination(
    termination_id: str,
    approved_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Approve a lease termination."""
    try:
        result = await crud.approve_termination(db_session, user_id, termination_id, approved_by)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Termination not found")
    except ConflictError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    logger.info("Lease termination approved", termination_id=termination_id)
    return result


@app.put("/terminations/{termination_id}/complete")
async def complete_termination(
    termination_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Complete a lease termination."""
    try:
        result = await crud.complete_termination(db_session, user_id, termination_id)
    except NotFoundError:
        raise HTTPException(status_code=404, detail="Termination not found")
    except ConflictError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    logger.info("Lease termination completed", termination_id=termination_id)
    return result


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
