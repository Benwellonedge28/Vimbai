"""
Vimbai Lease Termination Service
Manages lease terminations, early exit penalties, and settlement calculations.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "lease-termination-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8424"))

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

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class LeaseTermination(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    lease_id: str
    termination_date: datetime
    original_end_date: datetime
    remaining_payments: int
    remaining_payment_amount: float
    early_termination_penalty: float = 0.0
    settlement_amount: float = 0.0
    reason: str = ""
    status: str = "pending"  # pending, approved, completed, rejected
    approved_by: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    completed_at: Optional[datetime] = None


class TerminationSettlement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    termination_id: str
    remaining_lease_payments: float
    early_termination_penalty: float
    asset_return_value: float = 0.0
    net_settlement: float
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


terminations: List[LeaseTermination] = []
settlements: List[TerminationSettlement] = []


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
):
    """Initiate a lease termination."""
    settlement = remaining_payments * remaining_payment_amount + early_termination_penalty

    termination = LeaseTermination(
        lease_id=lease_id,
        termination_date=termination_date,
        original_end_date=original_end_date,
        remaining_payments=remaining_payments,
        remaining_payment_amount=remaining_payment_amount,
        early_termination_penalty=early_termination_penalty,
        settlement_amount=settlement,
        reason=reason,
    )
    terminations.append(termination)
    logger.info("Lease termination initiated", termination_id=termination.id, lease_id=lease_id)
    return termination


@app.get("/terminations", response_model=List[LeaseTermination])
async def list_terminations(status: Optional[str] = None):
    """List lease terminations."""
    if status:
        return [t for t in terminations if t.status == status]
    return terminations


@app.get("/terminations/{termination_id}", response_model=LeaseTermination)
async def get_termination(termination_id: str):
    """Get a specific termination."""
    term = next((t for t in terminations if t.id == termination_id), None)
    if not term:
        raise HTTPException(status_code=404, detail="Termination not found")
    return term


@app.post("/terminations/{termination_id}/settlement", response_model=TerminationSettlement)
async def calculate_settlement(termination_id: str, asset_return_value: float = 0.0):
    """Calculate and record the settlement for a lease termination."""
    term = next((t for t in terminations if t.id == termination_id), None)
    if not term:
        raise HTTPException(status_code=404, detail="Termination not found")

    remaining_payments_value = term.remaining_payments * term.remaining_payment_amount
    net_settlement = remaining_payments_value + term.early_termination_penalty - asset_return_value

    settlement = TerminationSettlement(
        termination_id=termination_id,
        remaining_lease_payments=remaining_payments_value,
        early_termination_penalty=term.early_termination_penalty,
        asset_return_value=asset_return_value,
        net_settlement=net_settlement,
    )
    settlements.append(settlement)
    logger.info("Settlement calculated", termination_id=termination_id, net_settlement=net_settlement)
    return settlement


@app.put("/terminations/{termination_id}/approve")
async def approve_termination(termination_id: str, approved_by: str):
    """Approve a lease termination."""
    term = next((t for t in terminations if t.id == termination_id), None)
    if not term:
        raise HTTPException(status_code=404, detail="Termination not found")
    if term.status != "pending":
        raise HTTPException(status_code=400, detail="Termination is not pending")

    term.status = "approved"
    term.approved_by = approved_by
    logger.info("Lease termination approved", termination_id=termination_id)
    return {"termination_id": termination_id, "status": "approved"}


@app.put("/terminations/{termination_id}/complete")
async def complete_termination(termination_id: str):
    """Complete a lease termination."""
    term = next((t for t in terminations if t.id == termination_id), None)
    if not term:
        raise HTTPException(status_code=404, detail="Termination not found")
    if term.status != "approved":
        raise HTTPException(status_code=400, detail="Termination must be approved first")

    term.status = "completed"
    term.completed_at = datetime.now(timezone.utc)
    logger.info("Lease termination completed", termination_id=termination_id)
    return {"termination_id": termination_id, "status": "completed"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
