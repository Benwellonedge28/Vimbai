"""
Vimbai Treasury Reporting Service
Generates treasury reports: cash position, FX exposure, debt portfolio, and liquidity.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "treasury-reporting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8262"))

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

app = FastAPI(title="Vimbai Treasury Reporting Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class TreasuryReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    report_type: str  # cash_position, fx_exposure, debt_portfolio, liquidity, investment_summary
    period: str  # YYYY-MM or YYYY-Qn
    data: Dict[str, Any] = {}
    summary: Dict[str, Any] = {}
    generated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    generated_by: str = ""


class CashPositionEntry(BaseModel):
    account_id: str
    account_name: str
    currency: str
    balance: float
    balance_usd: float = 0.0


class FXExposureEntry(BaseModel):
    currency_pair: str
    exposure_amount: float
    exposure_usd: float = 0.0
    hedge_ratio: float = 0.0
    unhedged_amount: float = 0.0


reports: List[TreasuryReport] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/reports/cash-position", response_model=TreasuryReport)
async def generate_cash_position(period: str, entries: List[CashPositionEntry], generated_by: str = ""):
    """Generate a cash position report."""
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
    reports.append(report)
    logger.info("Cash position report generated", report_id=report.id, period=period, total_usd=total_usd)
    return report


@app.post("/reports/fx-exposure", response_model=TreasuryReport)
async def generate_fx_exposure(period: str, entries: List[FXExposureEntry], generated_by: str = ""):
    """Generate an FX exposure report."""
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
    reports.append(report)
    logger.info("FX exposure report generated", report_id=report.id, period=period)
    return report


@app.post("/reports/debt-portfolio", response_model=TreasuryReport)
async def generate_debt_portfolio(
    period: str,
    total_debt: float,
    total_debt_usd: float,
    weighted_avg_rate: float,
    debt_instruments: List[Dict[str, Any]],
    generated_by: str = "",
):
    """Generate a debt portfolio report."""
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
    reports.append(report)
    logger.info("Debt portfolio report generated", report_id=report.id, period=period)
    return report


@app.get("/reports", response_model=List[TreasuryReport])
async def list_reports(report_type: Optional[str] = None, limit: int = 50):
    """List treasury reports."""
    result = reports
    if report_type:
        result = [r for r in result if r.report_type == report_type]
    return result[-limit:]


@app.get("/reports/{report_id}", response_model=TreasuryReport)
async def get_report(report_id: str):
    """Get a specific report."""
    report = next((r for r in reports if r.id == report_id), None)
    if not report:
        raise HTTPException(status_code=404, detail="Report not found")
    return report


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
