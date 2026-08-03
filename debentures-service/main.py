"""
Vimbai Debentures Service
Manages debenture issuance, interest, and redemption.
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

SERVICE_NAME = "debentures-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8058"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Debentures Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class DebentureClass(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    company_id: str
    nominal_value: float
    issue_price: float
    coupon_rate: float  # Annual interest rate as percentage
    interest_payment_frequency: str  # annual, semi_annual, quarterly, monthly
    maturity_date: datetime
    redemption_price: float
    convertibility: str = "none"  # none, convertible, optionally_convertible
    conversion_terms: Optional[str] = None
    debentures_issued: int = 0
    debentures_outstanding: int = 0


class DebentureIssue(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    debenture_class_id: str
    debentures_issued: int
    issue_date: datetime
    total_proceeds: float = 0
    discount_on_issue: float = 0
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class InterestPayment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    debenture_class_id: str
    period_start: datetime
    period_end: datetime
    debentures_outstanding: int
    interest_rate: float
    interest_amount: float = 0
    tax_deducted: float = 0
    net_payment: float = 0
    payment_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "accrued"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class RedemptionEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    debenture_class_id: str
    debentures_redeemed: int
    redemption_date: datetime
    redemption_price: float
    total_proceeds: float = 0
    premium_on_redemption: float = 0
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


debenture_classes: List[DebentureClass] = []
debenture_issues: List[DebentureIssue] = []
interest_payments: List[InterestPayment] = []
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Debentures management"}


@app.post("/classes/create")
async def create_debenture_class(
    name: str, company_id: str, nominal_value: float, issue_price: float,
    coupon_rate: float, interest_payment_frequency: str, maturity_date: datetime,
    redemption_price: float, convertibility: str = "none", conversion_terms: Optional[str] = None
):
    """Create a debenture class."""
    deb_class = DebentureClass(
        name=name, company_id=company_id, nominal_value=nominal_value,
        issue_price=issue_price, coupon_rate=coupon_rate,
        interest_payment_frequency=interest_payment_frequency, maturity_date=maturity_date,
        redemption_price=redemption_price, convertibility=convertibility,
        conversion_terms=conversion_terms
    )
    debenture_classes.append(deb_class)
    return deb_class


@app.post("/classes/{debenture_class_id}/issue")
async def issue_debentures(
    debenture_class_id: str, company_id: str, debentures_issued: int, issue_date: datetime
):
    """Issue debentures."""
    deb_class = next((d for d in debenture_classes if d.id == debenture_class_id), None)
    if not deb_class:
        return {"error": "Debenture class not found"}

    issue = DebentureIssue(
        company_id=company_id, debenture_class_id=debenture_class_id,
        debentures_issued=debentures_issued, issue_date=issue_date
    )
    issue.total_proceeds = debentures_issued * deb_class.issue_price
    issue.discount_on_issue = debentures_issued * (deb_class.nominal_value - deb_class.issue_price)

    deb_class.debentures_issued += debentures_issued
    deb_class.debentures_outstanding += debentures_issued

    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {debentures_issued} {deb_class.name} debentures",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": issue.total_proceeds, "credit": 0},
            {"account_code": "2330", "description": "Debenture Discount", "debit": issue.discount_on_issue, "credit": 0},
            {"account_code": "2320", "description": "Debenture Stock", "debit": 0, "credit": debentures_issued * deb_class.nominal_value},
        ],
        "reference": f"DEB-ISS-{issue.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    issue.journal_entry_id = result.get("id")
    debenture_issues.append(issue)

    return issue


@app.post("/classes/{debenture_class_id}/interest/accrue")
async def accrue_interest(
    debenture_class_id: str, company_id: str, period_start: datetime,
    period_end: datetime, debentures_outstanding: int
):
    """Accrue debenture interest."""
    deb_class = next((d for d in debenture_classes if d.id == debenture_class_id), None)
    if not deb_class:
        return {"error": "Debenture class not found"}

    interest = InterestPayment(
        company_id=company_id, debenture_class_id=debenture_class_id,
        period_start=period_start, period_end=period_end,
        debentures_outstanding=debentures_outstanding, interest_rate=deb_class.coupon_rate
    )

    # Calculate interest based on frequency
    if deb_class.interest_payment_frequency == "annual":
        periods = 1
    elif deb_class.interest_payment_frequency == "semi_annual":
        periods = 2
    elif deb_class.interest_payment_frequency == "quarterly":
        periods = 4
    else:  # monthly
        periods = 12

    annual_interest = debentures_outstanding * deb_class.nominal_value * (deb_class.coupon_rate / 100)
    interest.interest_amount = annual_interest / periods
    interest.tax_deducted = interest.interest_amount * 0.2  # Assuming 20% tax
    interest.net_payment = interest.interest_amount - interest.tax_deducted

    journal_entry = {
        "date": period_end,
        "description": f"Accrual of {deb_class.name} debenture interest",
        "entries": [
            {"account_code": "4100", "description": "Interest Expense", "debit": interest.interest_amount, "credit": 0},
            {"account_code": "2335", "description": "Interest Payable", "debit": 0, "credit": interest.interest_amount},
        ],
        "reference": f"DEB-INT-{interest.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    interest.journal_entry_id = result.get("id")
    interest_payments.append(interest)

    return interest


@app.post("/interest/{interest_id}/pay")
async def pay_interest(interest_id: str, payment_date: datetime):
    """Pay debenture interest."""
    interest = next((i for i in interest_payments if i.id == interest_id), None)
    if not interest:
        return {"error": "Interest not found"}

    journal_entry = {
        "date": payment_date,
        "description": "Payment of debenture interest",
        "entries": [
            {"account_code": "2335", "description": "Interest Payable", "debit": interest.interest_amount, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": interest.net_payment},
            {"account_code": "2200", "description": "Tax Payable", "debit": 0, "credit": interest.tax_deducted},
        ],
        "reference": f"DEB-INT-PAY-{interest_id[:8]}"
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    interest.payment_date = payment_date
    interest.status = "paid"

    return interest


@app.post("/classes/{debenture_class_id}/redeem")
async def redeem_debentures(
    debenture_class_id: str, company_id: str, debentures_redeemed: int,
    redemption_date: datetime
):
    """Redeem debentures."""
    deb_class = next((d for d in debenture_classes if d.id == debenture_class_id), None)
    if not deb_class:
        return {"error": "Debenture class not found"}

    redemption = RedemptionEntry(
        company_id=company_id, debenture_class_id=debenture_class_id,
        debentures_redeemed=debentures_redeemed, redemption_date=redemption_date,
        redemption_price=deb_class.redemption_price
    )
    redemption.total_proceeds = debentures_redeemed * deb_class.redemption_price
    redemption.premium_on_redemption = debentures_redeemed * (deb_class.redemption_price - deb_class.nominal_value)

    deb_class.debentures_outstanding -= debentures_redeemed

    journal_entry = {
        "date": redemption_date,
        "description": f"Redemption of {debentures_redeemed} {deb_class.name} debentures",
        "entries": [
            {"account_code": "2320", "description": "Debenture Stock", "debit": debentures_redeemed * deb_class.nominal_value, "credit": 0},
            {"account_code": "4100", "description": "Premium on Redemption", "debit": redemption.premium_on_redemption, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": redemption.total_proceeds},
        ],
        "reference": f"DEB-RED-{redemption.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    redemption.journal_entry_id = result.get("id")
    redemptions.append(redemption)

    return redemption


@app.get("/classes")
async def list_debenture_classes(company_id: Optional[str] = None):
    """List debenture classes."""
    result = debenture_classes
    if company_id:
        result = [d for d in result if d.company_id == company_id]
    return {"debenture_classes": result}


@app.get("/issues")
async def list_issues(company_id: Optional[str] = None):
    """List debenture issues."""
    result = debenture_issues
    if company_id:
        result = [i for i in result if i.company_id == company_id]
    return {"issues": result}


@app.get("/interest")
async def list_interest_payments(company_id: Optional[str] = None):
    """List interest payments."""
    result = interest_payments
    if company_id:
        result = [i for i in result if i.company_id == company_id]
    return {"interest_payments": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)