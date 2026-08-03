"""
Vimbai Retained Profits Service
Manages retained profits and profit appropriations.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "retained-profits-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8054"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Retained Profits Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ProfitAppropriation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    financial_year: str
    period_start: datetime
    period_end: datetime
    net_profit: float
    appropriations: Dict[str, float] = {}  # type -> amount
    total_appropriations: float = 0
    balance_carried_forward: float = 0
    opening_retained_earnings: float = 0
    closing_retained_earnings: float = 0
    journal_entry_id: Optional[str] = None
    status: str = "draft"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class InterimDividend(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    per_share_amount: float
    total_shares: int
    declaration_date: datetime
    payment_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "declared"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PriorYearAdjustment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    adjustment_type: str  # correction_of_error, change_in_accounting_policy
    description: str
    amount: float
    affected_year: str
    journal_entry_id: Optional[str] = None
    adjustment_date: datetime
    created_at: datetime = Field(default_factory=datetime.utcnow)


appropriations: List[ProfitAppropriation] = []
interim_dividends: List[InterimDividend] = []
prior_year_adjustments: List[PriorYearAdjustment] = []


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Retained profits management"}


@app.post("/appropriations/create")
async def create_appropriation(
    company_id: str, financial_year: str, period_start: datetime, period_end: datetime,
    net_profit: float, opening_retained_earnings: float,
    appropriations: Dict[str, float]  # e.g., {"dividends": 50000, "general_reserve": 20000}
):
    """Create profit appropriation statement."""
    appropriation = ProfitAppropriation(
        company_id=company_id, financial_year=financial_year, period_start=period_start,
        period_end=period_end, net_profit=net_profit, appropriations=appropriations,
        opening_retained_earnings=opening_retained_earnings
    )
    appropriation.total_appropriations = sum(appropriations.values())
    appropriation.balance_carried_forward = net_profit - appropriation.total_appropriations
    appropriation.closing_retained_earnings = opening_retained_earnings + appropriation.balance_carried_forward

    entries = [
        {"account_code": "3300", "description": "Retained Earnings", "debit": net_profit, "credit": 0},
    ]
    for alloc_type, amount in appropriations.items():
        entries.append({"account_code": "3300", "description": f"Retained Earnings", "debit": 0, "credit": amount})

    journal_entry = {
        "date": period_end,
        "description": f"Profit appropriation for {financial_year}",
        "entries": entries,
        "reference": f"APPROP-{appropriation.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    appropriation.journal_entry_id = result.get("id")
    appropriation.status = "approved"
    appropriations.append(appropriation)

    return appropriation


@app.post("/interim-dividends/declare")
async def declare_interim_dividend(
    company_id: str, amount: float, per_share_amount: float, total_shares: int,
    declaration_date: datetime
):
    """Declare interim dividend."""
    dividend = InterimDividend(
        company_id=company_id, amount=amount, per_share_amount=per_share_amount,
        total_shares=total_shares, declaration_date=declaration_date
    )

    journal_entry = {
        "date": declaration_date,
        "description": "Declaration of interim dividend",
        "entries": [
            {"account_code": "3300", "description": "Retained Earnings", "debit": amount, "credit": 0},
            {"account_code": "2310", "description": "Dividend Payable", "debit": 0, "credit": amount},
        ],
        "reference": f"INTERIM-DIV-{dividend.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.journal_entry_id = result.get("id")
    interim_dividends.append(dividend)

    return dividend


@app.post("/interim-dividends/{dividend_id}/pay")
async def pay_interim_dividend(dividend_id: str, payment_date: datetime):
    """Pay interim dividend."""
    dividend = next((d for d in interim_dividends if d.id == dividend_id), None)
    if not dividend:
        return {"error": "Dividend not found"}

    journal_entry = {
        "date": payment_date,
        "description": "Payment of interim dividend",
        "entries": [
            {"account_code": "2310", "description": "Dividend Payable", "debit": dividend.amount, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": dividend.amount},
        ],
        "reference": f"INTERIM-DIV-PAY-{dividend_id[:8]}"
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.payment_date = payment_date
    dividend.status = "paid"

    return dividend


@app.post("/adjustments/create")
async def create_prior_year_adjustment(
    company_id: str, adjustment_type: str, description: str, amount: float,
    affected_year: str, adjustment_date: Optional[datetime] = None
):
    """Create prior year adjustment."""
    if adjustment_date is None:
        adjustment_date = datetime.utcnow()

    adjustment = PriorYearAdjustment(
        company_id=company_id, adjustment_type=adjustment_type, description=description,
        amount=amount, affected_year=affected_year, adjustment_date=adjustment_date
    )

    # Prior year adjustments affect opening retained earnings
    journal_entry = {
        "date": adjustment_date,
        "description": f"Prior year adjustment: {description}",
        "entries": [
            {"account_code": "3300", "description": "Retained Earnings", "debit": amount if amount > 0 else 0, "credit": abs(amount) if amount < 0 else 0},
            {"account_code": "1100", "description": "Trade Receivables", "debit": abs(amount) if amount < 0 else 0, "credit": amount if amount > 0 else 0},
        ],
        "reference": f"PYA-{adjustment.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    adjustment.journal_entry_id = result.get("id")
    prior_year_adjustments.append(adjustment)

    return adjustment


@app.get("/appropriations")
async def list_appropriations(company_id: Optional[str] = None, financial_year: Optional[str] = None):
    """List profit appropriations."""
    result = appropriations
    if company_id:
        result = [a for a in result if a.company_id == company_id]
    if financial_year:
        result = [a for a in result if a.financial_year == financial_year]
    return {"appropriations": result}


@app.get("/interim-dividends")
async def list_interim_dividends(company_id: Optional[str] = None):
    """List interim dividends."""
    result = interim_dividends
    if company_id:
        result = [d for d in result if d.company_id == company_id]
    return {"interim_dividends": result}


@app.get("/adjustments")
async def list_adjustments(company_id: Optional[str] = None):
    """List prior year adjustments."""
    result = prior_year_adjustments
    if company_id:
        result = [a for a in result if a.company_id == company_id]
    return {"adjustments": result}


@app.get("/summary/{company_id}")
async def get_retained_earnings_summary(company_id: str):
    """Get retained earnings summary."""
    company_appropriations = [a for a in appropriations if a.company_id == company_id]
    company_adjustments = [a for a in prior_year_adjustments if a.company_id == company_id]

    total_carried_forward = sum(a.balance_carried_forward for a in company_appropriations)
    total_adjustments = sum(a.amount for a in company_adjustments)

    return {
        "company_id": company_id,
        "total_carried_forward": total_carried_forward,
        "total_prior_year_adjustments": total_adjustments,
        "net_retained_earnings": total_carried_forward + total_adjustments,
        "last_appropriation": company_appropriations[-1] if company_appropriations else None
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)