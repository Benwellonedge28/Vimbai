"""
FinAcc Capital Redemption Reserve Service
Manages capital redemption reserve operations.
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

SERVICE_NAME = "capital-redemption-reserve-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8055"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Capital Redemption Reserve Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class RedemptionTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    share_class: str  # preference, ordinary
    shares_redeemed: int
    redemption_price: float
    nominal_value: float
    total_proceeds: float = 0
    redemption_reserve_amount: float = 0  # proceeds - nominal
    redemption_date: datetime
    source_account: str  # proceeds, fresh_issue, bonus_issue
    journal_entry_id: Optional[str] = None
    status: str = "completed"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CRRCreation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    source: str  # share_redemption, capital_reduction, fresh_issue
    description: str
    creation_date: datetime
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CRRUtilization(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    utilization_type: str  # bonus_issue, write_off, transfer_general
    description: str
    utilization_date: datetime
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


redemption_transactions: List[RedemptionTransaction] = []
crr_creations: List[CRRCreation] = []
crr_utilizations: List[CRRUtilization] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Capital redemption reserve management"}


@app.post("/redemptions/record")
async def record_redemption(
    company_id: str, share_class: str, shares_redeemed: int, redemption_price: float,
    nominal_value: float, redemption_date: datetime, source_account: str
):
    """Record share redemption creating CRR."""
    transaction = RedemptionTransaction(
        company_id=company_id, share_class=share_class, shares_redeemed=shares_redeemed,
        redemption_price=redemption_price, nominal_value=nominal_value,
        redemption_date=redemption_date, source_account=source_account
    )
    transaction.total_proceeds = shares_redeemed * redemption_price
    transaction.redemption_reserve_amount = transaction.total_proceeds - (shares_redeemed * nominal_value)

    capital_account = "3205" if share_class == "preference" else "3200"

    journal_entry = {
        "date": redemption_date,
        "description": f"Redemption of {shares_redeemed} {share_class} shares - CRR created",
        "entries": [
            {"account_code": capital_account, "description": f"{share_class.title()} Share Capital", "debit": shares_redeemed * nominal_value, "credit": 0},
            {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": transaction.redemption_reserve_amount, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": transaction.total_proceeds},
        ],
        "reference": f"CRR-RED-{transaction.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    transaction.journal_entry_id = result.get("id")
    redemption_transactions.append(transaction)

    return transaction


@app.post("/creations/create")
async def create_crr(
    company_id: str, amount: float, source: str, description: str,
    creation_date: Optional[datetime] = None
):
    """Manually create CRR (e.g., from capital reduction)."""
    if creation_date is None:
        creation_date = datetime.utcnow()

    crr = CRRCreation(
        company_id=company_id, amount=amount, source=source,
        description=description, creation_date=creation_date
    )

    journal_entry = {
        "date": creation_date,
        "description": f"Creation of Capital Redemption Reserve: {description}",
        "entries": [
            {"account_code": "3300", "description": "Retained Earnings / P&L", "debit": amount, "credit": 0},
            {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": 0, "credit": amount},
        ],
        "reference": f"CRR-CREATE-{crr.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    crr.journal_entry_id = result.get("id")
    crr_creations.append(crr)

    return crr


@app.post("/utilizations/record")
async def utilize_crr(
    company_id: str, amount: float, utilization_type: str, description: str,
    utilization_date: Optional[datetime] = None
):
    """Utilize CRR (e.g., for bonus issue)."""
    if utilization_date is None:
        utilization_date = datetime.utcnow()

    utilization = CRRUtilization(
        company_id=company_id, amount=amount, utilization_type=utilization_type,
        description=description, utilization_date=utilization_date
    )

    if utilization_type == "bonus_issue":
        journal_entry = {
            "date": utilization_date,
            "description": f"CRR utilized for bonus issue: {description}",
            "entries": [
                {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": amount, "credit": 0},
                {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": amount},
            ],
            "reference": f"CRR-UTIL-{utilization.id[:8]}"
        }
    elif utilization_type == "write_off":
        journal_entry = {
            "date": utilization_date,
            "description": f"CRR written off: {description}",
            "entries": [
                {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": amount, "credit": 0},
                {"account_code": "3300", "description": "Retained Earnings", "debit": 0, "credit": amount},
            ],
            "reference": f"CRR-UTIL-{utilization.id[:8]}"
        }
    else:
        journal_entry = {
            "date": utilization_date,
            "description": f"CRR transferred: {description}",
            "entries": [
                {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": amount, "credit": 0},
                {"account_code": "3310", "description": "General Reserve", "debit": 0, "credit": amount},
            ],
            "reference": f"CRR-UTIL-{utilization.id[:8]}"
        }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    utilization.journal_entry_id = result.get("id")
    crr_utilizations.append(utilization)

    return utilization


@app.get("/redemptions")
async def list_redemptions(company_id: Optional[str] = None):
    """List redemption transactions."""
    result = redemption_transactions
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    return {"redemptions": result}


@app.get("/creations")
async def list_creations(company_id: Optional[str] = None):
    """List CRR creations."""
    result = crr_creations
    if company_id:
        result = [c for c in result if c.company_id == company_id]
    return {"creations": result}


@app.get("/utilizations")
async def list_utilizations(company_id: Optional[str] = None):
    """List CRR utilizations."""
    result = crr_utilizations
    if company_id:
        result = [u for u in result if u.company_id == company_id]
    return {"utilizations": result}


@app.get("/summary/{company_id}")
async def get_crr_summary(company_id: str):
    """Get CRR balance summary."""
    company_redemptions = [r for r in redemption_transactions if r.company_id == company_id]
    company_creations = [c for c in crr_creations if c.company_id == company_id]
    company_utilizations = [u for u in crr_utilizations if u.company_id == company_id]

    total_created = sum(r.redemption_reserve_amount for r in company_redemptions) + sum(c.amount for c in company_creations)
    total_utilized = sum(u.amount for u in company_utilizations)

    return {
        "company_id": company_id,
        "total_created": total_created,
        "total_utilized": total_utilized,
        "current_balance": total_created - total_utilized,
        "transaction_count": len(company_redemptions) + len(company_creations) + len(company_utilizations)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)