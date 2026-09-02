"""
Vimbai Preference Shares Service
Manages preference share issuance and dividends.
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

SERVICE_NAME = "preference-shares-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8052"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

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

app = FastAPI(title="Vimbai Preference Shares Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class DividendPriority(str):
    PREFERENCE = "preference"
    MIXED = "mixed"


class PreferenceShareClass(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    company_id: str
    nominal_value: float
    issue_price: float
    fixed_dividend_rate: float  # Annual percentage rate
    dividend_type: str  # cumulative, non_cumulative
    participation_rights: str  # full, limited, none
    liquidation_priority: int  # 1 = highest priority
    redemption_terms: Optional[str] = None
    conversion_terms: Optional[str] = None
    shares_issued: int = 0
    shares_outstanding: int = 0


class PreferenceDividend(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    company_id: str
    dividend_type: str  # fixed, participating_surplus
    per_share_amount: float
    total_shares: int
    total_dividend: float = 0
    preference_arears: float = 0  # For cumulative shares
    record_date: datetime
    payment_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "declared"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class RedemptionEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    shares_redeemed: int
    redemption_price: float
    total_proceeds: float = 0
    redemption_date: datetime
    journal_entry_id: Optional[str] = None
    status: str = "completed"
    created_at: datetime = Field(default_factory=datetime.utcnow)


share_classes: List[PreferenceShareClass] = []
preference_dividends: List[PreferenceDividend] = []
redemptions: List[RedemptionEntry] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Preference shares management"}


@app.post("/classes/create")
async def create_share_class(
    name: str,
    company_id: str,
    nominal_value: float,
    issue_price: float,
    fixed_dividend_rate: float,
    dividend_type: str,
    participation_rights: str,
    liquidation_priority: int,
    redemption_terms: Optional[str] = None,
    conversion_terms: Optional[str] = None,
):
    """Create a preference share class."""
    share_class = PreferenceShareClass(
        name=name,
        company_id=company_id,
        nominal_value=nominal_value,
        issue_price=issue_price,
        fixed_dividend_rate=fixed_dividend_rate,
        dividend_type=dividend_type,
        participation_rights=participation_rights,
        liquidation_priority=liquidation_priority,
        redemption_terms=redemption_terms,
        conversion_terms=conversion_terms,
    )
    share_classes.append(share_class)
    return share_class


@app.post("/classes/{share_class_id}/issue")
async def issue_preference_shares(share_class_id: str, shares_issued: int, issue_date: datetime):
    """Issue preference shares of a given class."""
    share_class = next((s for s in share_classes if s.id == share_class_id), None)
    if not share_class:
        return {"error": "Share class not found"}

    share_class.shares_issued += shares_issued
    share_class.shares_outstanding += shares_issued

    proceeds = shares_issued * share_class.issue_price
    nominal = shares_issued * share_class.nominal_value
    premium = proceeds - nominal

    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {shares_issued} {share_class.name} preference shares",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": proceeds, "credit": 0},
            {"account_code": "3205", "description": "Preference Share Capital", "debit": 0, "credit": nominal},
            {"account_code": "3215", "description": "Preference Share Premium", "debit": 0, "credit": premium},
        ],
        "reference": f"PREF-ISS-{share_class_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    return share_class


@app.post("/classes/{share_class_id}/dividends/declare")
async def declare_preference_dividend(
    share_class_id: str,
    company_id: str,
    per_share_amount: float,
    total_shares: int,
    preference_arears: float,
    record_date: datetime,
):
    """Declare preference share dividend."""
    share_class = next((s for s in share_classes if s.id == share_class_id), None)
    if not share_class:
        return {"error": "Share class not found"}

    dividend = PreferenceDividend(
        share_class_id=share_class_id,
        company_id=company_id,
        dividend_type="fixed",
        per_share_amount=per_share_amount,
        total_shares=total_shares,
        preference_arears=preference_arears,
        record_date=record_date,
    )
    dividend.total_dividend = (per_share_amount * total_shares) + preference_arears

    journal_entry = {
        "date": record_date,
        "description": f"Declaration of {share_class.name} preference dividend",
        "entries": [
            {"account_code": "3300", "description": "Retained Earnings", "debit": dividend.total_dividend, "credit": 0},
            {
                "account_code": "2315",
                "description": "Preference Dividend Payable",
                "debit": 0,
                "credit": dividend.total_dividend,
            },
        ],
        "reference": f"PREF-DIV-{dividend.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.journal_entry_id = result.get("id")
    preference_dividends.append(dividend)

    return dividend


@app.post("/classes/{share_class_id}/dividends/{dividend_id}/pay")
async def pay_preference_dividend(share_class_id: str, dividend_id: str, payment_date: datetime):
    """Pay preference dividend."""
    dividend = next((d for d in preference_dividends if d.id == dividend_id), None)
    if not dividend:
        return {"error": "Dividend not found"}

    journal_entry = {
        "date": payment_date,
        "description": "Payment of preference dividend",
        "entries": [
            {
                "account_code": "2315",
                "description": "Preference Dividend Payable",
                "debit": dividend.total_dividend,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": dividend.total_dividend},
        ],
        "reference": f"PREF-DIV-PAY-{dividend_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.payment_date = payment_date
    dividend.status = "paid"

    return dividend


@app.post("/classes/{share_class_id}/redeem")
async def redeem_preference_shares(
    share_class_id: str, shares_redeemed: int, redemption_price: float, redemption_date: datetime
):
    """Redeem preference shares."""
    share_class = next((s for s in share_classes if s.id == share_class_id), None)
    if not share_class:
        return {"error": "Share class not found"}

    redemption = RedemptionEntry(
        share_class_id=share_class_id,
        shares_redeemed=shares_redeemed,
        redemption_price=redemption_price,
        redemption_date=redemption_date,
    )
    redemption.total_proceeds = shares_redeemed * redemption_price

    share_class.shares_outstanding -= shares_redeemed

    nominal = shares_redeemed * share_class.nominal_value

    journal_entry = {
        "date": redemption_date,
        "description": f"Redemption of {shares_redeemed} {share_class.name} preference shares",
        "entries": [
            {"account_code": "3205", "description": "Preference Share Capital", "debit": nominal, "credit": 0},
            {
                "account_code": "3220",
                "description": "Capital Redemption Reserve",
                "debit": redemption.total_proceeds - nominal,
                "credit": 0,
            },
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": redemption.total_proceeds},
        ],
        "reference": f"PREF-RED-{redemption.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    redemption.journal_entry_id = result.get("id")
    redemptions.append(redemption)

    return redemption


@app.get("/classes")
async def list_share_classes(company_id: Optional[str] = None):
    """List preference share classes."""
    result = share_classes
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    return {"share_classes": result}


@app.get("/dividends")
async def list_dividends(company_id: Optional[str] = None):
    """List preference dividends."""
    result = preference_dividends
    if company_id:
        result = [d for d in result if d.company_id == company_id]
    return {"dividends": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
