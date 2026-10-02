"""Vimbai Petty Cash Book Service - petty cash management with full audit trail.

Supports multiple petty cash funds, reimbursement workflows, and integration
with the main accounting system. Durable records (funds, transactions,
vouchers, replenishments) persist in Neo4j, stamped with the caller and the
Book context (X-Book-ID verified upstream by the API gateway).

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "petty_cash_service" not in _sys.modules or not hasattr(_sys.modules.get("petty_cash_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("petty_cash_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["petty_cash_service"] = _pkg
    _sys.modules["petty_cash_service"].__path__ = [_HERE]

from datetime import datetime
from typing import Optional

from fastapi import Depends, FastAPI, Request
from neo4j import AsyncSession
from petty_cash_service import crud
from petty_cash_service.dependencies import book_id_var, get_db_session, get_user_id
from petty_cash_service.exceptions import PettyCashError
from petty_cash_service.models import (
    PaymentCategory,
    PettyCashFund,
    PettyCashReplenishment,
    PettyCashStatus,
    PettyCashTransaction,
    PettyCashVoucher,
    ReimbursementStatus,
    TransactionType,
)

SERVICE_NAME = "petty-cash-service"
app = FastAPI(
    title="Vimbai Petty Cash Book Service",
    description="Comprehensive petty cash management with fund tracking, reimbursement workflows, and accounting integration",
    version="2.0.0",
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None
    import logging

    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")


@app.exception_handler(PettyCashError)
async def _petty_cash_error(request: Request, exc: PettyCashError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
async def health_check():
    """Health check endpoint"""
    return {"status": "healthy", "service": "petty-cash", "version": "2.0.0"}


# --- Fund Management ---


@app.post("/funds")
async def create_petty_cash_fund(
    fund: PettyCashFund,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new petty cash fund"""
    return await crud.create_fund(db_session, user_id, fund)


@app.get("/funds")
async def list_petty_cash_funds(
    status: Optional[PettyCashStatus] = None,
    custodian_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List petty cash funds (caller-owned, Book-visible)"""
    return await crud.list_funds(db_session, user_id, status=status, custodian_id=custodian_id)


@app.get("/funds/{fund_id}")
async def get_petty_cash_fund(
    fund_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get petty cash fund details"""
    return await crud.get_fund(db_session, user_id, fund_id)


@app.put("/funds/{fund_id}")
async def update_petty_cash_fund(
    fund_id: str,
    fund: PettyCashFund,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update petty cash fund"""
    return await crud.update_fund(db_session, user_id, fund_id, fund)


@app.post("/funds/{fund_id}/close")
async def close_petty_cash_fund(
    fund_id: str,
    closed_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Close a petty cash fund"""
    await crud.close_fund(db_session, user_id, fund_id)
    return {"status": "closed", "fund_id": fund_id, "closed_by": closed_by}


# --- Transaction Management ---


@app.post("/transactions")
async def create_petty_cash_transaction(
    transaction: PettyCashTransaction,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create petty cash transaction"""
    return await crud.create_transaction(db_session, user_id, transaction)


@app.get("/transactions")
async def list_petty_cash_transactions(
    fund_id: Optional[str] = None,
    transaction_type: Optional[TransactionType] = None,
    category: Optional[PaymentCategory] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List petty cash transactions (caller-owned, Book-visible)"""
    return await crud.list_transactions(
        db_session,
        user_id,
        fund_id=fund_id,
        transaction_type=transaction_type,
        category=category,
        start_date=start_date,
        end_date=end_date,
        limit=limit,
    )


@app.get("/transactions/{transaction_id}")
async def get_petty_cash_transaction(
    transaction_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get transaction details"""
    return await crud.get_transaction(db_session, user_id, transaction_id)


@app.get("/funds/{fund_id}/balance")
async def get_fund_balance(
    fund_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get current fund balance"""
    return await crud.fund_balance(db_session, user_id, fund_id)


@app.get("/funds/{fund_id}/summary")
async def get_fund_summary(
    fund_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get fund summary"""
    return await crud.fund_summary(db_session, user_id, fund_id)


# --- Voucher Management ---


@app.post("/vouchers")
async def create_petty_cash_voucher(
    voucher: PettyCashVoucher,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create petty cash voucher (creates the corresponding payment transaction)"""
    return await crud.create_voucher(db_session, user_id, voucher)


@app.get("/vouchers")
async def list_petty_cash_vouchers(
    fund_id: Optional[str] = None,
    status: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List petty cash vouchers (caller-owned, Book-visible)"""
    return await crud.list_vouchers(
        db_session, user_id, fund_id=fund_id, status=status, start_date=start_date, end_date=end_date
    )


@app.put("/vouchers/{voucher_id}/approve")
async def approve_voucher(
    voucher_id: str,
    approved_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Approve a voucher"""
    return await crud.approve_voucher(db_session, user_id, voucher_id, approved_by)


# --- Replenishment ---


@app.post("/replenishments")
async def create_replenishment(
    replenishment: PettyCashReplenishment,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create replenishment request"""
    return await crud.create_replenishment(db_session, user_id, replenishment)


@app.get("/replenishments")
async def list_replenishments(
    fund_id: Optional[str] = None,
    status: Optional[ReimbursementStatus] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List replenishment requests (caller-owned, Book-visible)"""
    return await crud.list_replenishments(db_session, user_id, fund_id=fund_id, status=status)


@app.put("/replenishments/{replenishment_id}/approve")
async def approve_replenishment(
    replenishment_id: str,
    approved_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Approve replenishment"""
    return await crud.approve_replenishment(db_session, user_id, replenishment_id, approved_by)


# --- Reports ---


@app.get("/reports/fund-report/{fund_id}")
async def get_fund_report(
    fund_id: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate fund report"""
    return await crud.fund_report(db_session, user_id, fund_id, start_date, end_date)


@app.get("/reports/cash-position")
async def get_cash_position(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cash position across all caller funds"""
    return await crud.cash_position(db_session, user_id)


@app.get("/reports/category-summary")
async def get_category_summary(
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get summary by payment category"""
    return await crud.category_summary(db_session, user_id, start_date, end_date)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8097)
