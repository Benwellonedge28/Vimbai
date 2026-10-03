"""
Vimbai Treasury Reporting Service
Generates treasury reports: cash position, FX exposure, debt portfolio,
and liquidity. Reports persist in Neo4j, caller-owned (X-User-Id) and
Book-gated (X-Book-ID, verified upstream by the API gateway).

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import logging
import os as _os
import sys as _sys
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "treasury_reporting_service" not in _sys.modules or not hasattr(
    _sys.modules.get("treasury_reporting_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("treasury_reporting_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["treasury_reporting_service"] = _pkg
    _sys.modules["treasury_reporting_service"].__path__ = [_HERE]

from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from treasury_reporting_service import crud
from treasury_reporting_service.dependencies import book_id_var, get_db_session, get_user_id
from treasury_reporting_service.models import CashPositionEntry, FXExposureEntry, TreasuryReport

SERVICE_NAME = "treasury-reporting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8262"))

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

app = FastAPI(title="Vimbai Treasury Reporting Service", version=SERVICE_VERSION, docs_url="/docs")
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


@app.post("/reports/cash-position", response_model=TreasuryReport)
async def generate_cash_position(
    period: str,
    entries: List[CashPositionEntry],
    generated_by: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate a cash position report (persisted to the caller's set)."""
    total_usd = sum(e.balance_usd for e in entries)
    by_currency: Dict[str, float] = {}
    for e in entries:
        by_currency[e.currency] = by_currency.get(e.currency, 0) + e.balance

    report = TreasuryReport(
        report_type="cash_position",
        period=period,
        data={"entries": [e.model_dump() for e in entries]},
        summary={
            "total_cash_usd": total_usd,
            "total_accounts": len(entries),
            "by_currency": by_currency,
        },
        generated_by=generated_by,
    )
    saved = await crud.create_report(db_session, user_id, report)
    logger.info("Cash position report generated", report_id=saved.id, period=period, total_usd=total_usd)
    return saved


@app.post("/reports/fx-exposure", response_model=TreasuryReport)
async def generate_fx_exposure(
    period: str,
    entries: List[FXExposureEntry],
    generated_by: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate an FX exposure report (persisted to the caller's set)."""
    total_exposure_usd = sum(e.exposure_usd for e in entries)
    total_unhedged = sum(e.unhedged_amount for e in entries)
    avg_hedge_ratio = sum(e.hedge_ratio for e in entries) / len(entries) if entries else 0

    report = TreasuryReport(
        report_type="fx_exposure",
        period=period,
        data={"entries": [e.model_dump() for e in entries]},
        summary={
            "total_exposure_usd": total_exposure_usd,
            "total_unhedged_usd": total_unhedged,
            "average_hedge_ratio": avg_hedge_ratio,
            "currency_pairs": len(entries),
        },
        generated_by=generated_by,
    )
    saved = await crud.create_report(db_session, user_id, report)
    logger.info("FX exposure report generated", report_id=saved.id, period=period)
    return saved


@app.post("/reports/debt-portfolio", response_model=TreasuryReport)
async def generate_debt_portfolio(
    period: str,
    total_debt: float,
    total_debt_usd: float,
    weighted_avg_rate: float,
    debt_instruments: List[Dict[str, Any]],
    generated_by: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate a debt portfolio report (persisted to the caller's set)."""
    report = TreasuryReport(
        report_type="debt_portfolio",
        period=period,
        data={"instruments": debt_instruments},
        summary={
            "total_debt": total_debt,
            "total_debt_usd": total_debt_usd,
            "weighted_avg_rate": weighted_avg_rate,
            "instrument_count": len(debt_instruments),
        },
        generated_by=generated_by,
    )
    saved = await crud.create_report(db_session, user_id, report)
    logger.info("Debt portfolio report generated", report_id=saved.id, period=period)
    return saved


@app.get("/reports", response_model=List[TreasuryReport])
async def list_reports(
    report_type: Optional[str] = None,
    limit: int = 50,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List treasury reports (caller's own Book-visible set)."""
    result = await crud.list_reports(db_session, user_id)
    if report_type:
        result = [r for r in result if r.report_type == report_type]
    # original semantics: last N entries in insertion order
    return result[-limit:]


@app.get("/reports/{report_id}", response_model=TreasuryReport)
async def get_report(
    report_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific report (caller's own Book-visible set)."""
    report = await crud.get_report(db_session, user_id, report_id)
    if not report:
        raise HTTPException(status_code=404, detail="Report not found")
    return report


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
