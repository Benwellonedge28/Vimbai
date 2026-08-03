"""
Vimbai Share Premium Service
Manages share premium account operations.
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

SERVICE_NAME = "share-premium-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8056"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Share Premium Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class SharePremiumEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    entry_type: str  # issue, conversion, reorganization, write_off
    shares_issued: int = 0
    nominal_value: float = 0
    issue_price: float = 0
    premium_amount: float = 0
    share_class: str = "ordinary"
    reference_id: str  # ID of the related share issue
    journal_entry_id: Optional[str] = None
    entry_date: datetime
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PremiumUtilization(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    amount: float
    utilization_type: str  # bonus_issue, write_off, merger_expenses, legal_costs
    description: str
    journal_entry_id: Optional[str] = None
    utilization_date: datetime
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PremiumAdjustment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    adjustment_type: str  # correction, reclassification
    original_amount: float
    adjustment_amount: float
    description: str
    journal_entry_id: Optional[str] = None
    adjustment_date: datetime
    created_at: datetime = Field(default_factory=datetime.utcnow)


share_premium_entries: List[SharePremiumEntry] = []
premium_utilizations: List[PremiumUtilization] = []
premium_adjustments: List[PremiumAdjustment] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Share premium management"}


@app.post("/entries/record")
async def record_share_premium(
    company_id: str, entry_type: str, shares_issued: int, nominal_value: float,
    issue_price: float, share_class: str, reference_id: str, entry_date: Optional[datetime] = None
):
    """Record share premium from share issuance."""
    if entry_date is None:
        entry_date = datetime.utcnow()

    premium_amount = (issue_price - nominal_value) * shares_issued

    entry = SharePremiumEntry(
        company_id=company_id, entry_type=entry_type, shares_issued=shares_issued,
        nominal_value=nominal_value, issue_price=issue_price,
        premium_amount=premium_amount, share_class=share_class,
        reference_id=reference_id, entry_date=entry_date
    )

    journal_entry = {
        "date": entry_date,
        "description": f"Share premium from issue of {shares_issued} {share_class} shares",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": shares_issued * issue_price, "credit": 0},
            {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": shares_issued * nominal_value},
            {"account_code": "3210", "description": "Share Premium", "debit": 0, "credit": premium_amount},
        ],
        "reference": f"SP-{entry.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    entry.journal_entry_id = result.get("id")
    share_premium_entries.append(entry)

    return entry


@app.post("/utilizations/record")
async def utilize_premium(
    company_id: str, amount: float, utilization_type: str, description: str,
    utilization_date: Optional[datetime] = None
):
    """Utilize share premium account."""
    if utilization_date is None:
        utilization_date = datetime.utcnow()

    utilization = PremiumUtilization(
        company_id=company_id, amount=amount, utilization_type=utilization_type,
        description=description, utilization_date=utilization_date
    )

    if utilization_type == "bonus_issue":
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium utilized for bonus issue: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}"
        }
    elif utilization_type == "write_off":
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium written off: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "3300", "description": "Retained Earnings", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}"
        }
    elif utilization_type == "merger_expenses":
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium for merger expenses: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "4100", "description": "Administrative Expenses", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}"
        }
    else:  # legal_costs
        journal_entry = {
            "date": utilization_date,
            "description": f"Share premium for legal costs: {description}",
            "entries": [
                {"account_code": "3210", "description": "Share Premium", "debit": amount, "credit": 0},
                {"account_code": "4100", "description": "Legal Expenses", "debit": 0, "credit": amount},
            ],
            "reference": f"SP-UTIL-{utilization.id[:8]}"
        }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    utilization.journal_entry_id = result.get("id")
    premium_utilizations.append(utilization)

    return utilization


@app.post("/adjustments/create")
async def adjust_premium(
    company_id: str, adjustment_type: str, original_amount: float,
    adjustment_amount: float, description: str,
    adjustment_date: Optional[datetime] = None
):
    """Adjust share premium (correction or reclassification)."""
    if adjustment_date is None:
        adjustment_date = datetime.utcnow()

    adjustment = PremiumAdjustment(
        company_id=company_id, adjustment_type=adjustment_type,
        original_amount=original_amount, adjustment_amount=adjustment_amount,
        description=description, adjustment_date=adjustment_date
    )

    journal_entry = {
        "date": adjustment_date,
        "description": f"Share premium adjustment: {description}",
        "entries": [
            {"account_code": "3210", "description": "Share Premium",
             "debit": abs(adjustment_amount) if adjustment_amount < 0 else 0,
             "credit": abs(adjustment_amount) if adjustment_amount > 0 else 0},
            {"account_code": "3300", "description": "Retained Earnings",
             "debit": abs(adjustment_amount) if adjustment_amount > 0 else 0,
             "credit": abs(adjustment_amount) if adjustment_amount < 0 else 0},
        ],
        "reference": f"SP-ADJ-{adjustment.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    adjustment.journal_entry_id = result.get("id")
    premium_adjustments.append(adjustment)

    return adjustment


@app.get("/entries")
async def list_entries(company_id: Optional[str] = None):
    """List share premium entries."""
    result = share_premium_entries
    if company_id:
        result = [e for e in result if e.company_id == company_id]
    return {"entries": result}


@app.get("/utilizations")
async def list_utilizations(company_id: Optional[str] = None):
    """List premium utilizations."""
    result = premium_utilizations
    if company_id:
        result = [u for u in result if u.company_id == company_id]
    return {"utilizations": result}


@app.get("/summary/{company_id}")
async def get_premium_summary(company_id: str):
    """Get share premium summary."""
    company_entries = [e for e in share_premium_entries if e.company_id == company_id]
    company_utilizations = [u for u in premium_utilizations if u.company_id == company_id]
    company_adjustments = [a for a in premium_adjustments if a.company_id == company_id]

    total_received = sum(e.premium_amount for e in company_entries)
    total_utilized = sum(u.amount for u in company_utilizations)
    total_adjusted = sum(a.adjustment_amount for a in company_adjustments)

    return {
        "company_id": company_id,
        "total_premium_received": total_received,
        "total_utilized": total_utilized,
        "total_adjusted": total_adjusted,
        "current_balance": total_received - total_utilized + total_adjusted
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)