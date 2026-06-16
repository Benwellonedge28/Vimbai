"""
FinAcc Partnership Changes Service
Manages admission, retirement, death, and insolvency of partners.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "partnership-changes-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8043"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Partnership Changes Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ChangeType(str, Enum):
    ADMISSION = "admission"
    RETIREMENT = "retirement"
    DEATH = "death"
    INSOLVENCY = "insolvency"
    EXPULSION = "expulsion"


class PartnerChange(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partnership_id: str
    change_type: ChangeType
    partner_id: str
    partner_name: str
    effective_date: datetime
    capital_balance: float = 0
    current_account_balance: float = 0
    total_payable: float = 0
    goodwill_amount: float = 0
    payment_method: str = "cash"  # cash, assets, mixed
    settlement_status: str = "pending"
    journal_entry_id: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class AdmissionDetails(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    new_partner_id: str
    new_partner_name: str
    capital_contribution: float
    goodwill_paid: float = 0
    premium_distribution: Dict[str, float] = {}  # partner_id -> amount
    new_profit_sharing_ratios: Dict[str, float] = {}
    revaluation_required: bool = False
    revaluation_amount: float = 0
    admission_date: datetime
    journal_entry_ids: List[str] = []


changes: List[PartnerChange] = []
admissions: List[AdmissionDetails] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Partnership changes management"}


@app.post("/changes/retirement")
async def record_retirement(
    partnership_id: str, partner_id: str, partner_name: str, effective_date: datetime,
    capital_balance: float, current_account_balance: float, goodwill_amount: float = 0, payment_method: str = "cash"
):
    """Record partner retirement."""
    total_payable = capital_balance + current_account_balance + goodwill_amount

    change = PartnerChange(
        partnership_id=partnership_id, change_type=ChangeType.RETIREMENT,
        partner_id=partner_id, partner_name=partner_name, effective_date=effective_date,
        capital_balance=capital_balance, current_account_balance=current_account_balance,
        total_payable=total_payable, goodwill_amount=goodwill_amount, payment_method=payment_method
    )
    changes.append(change)

    # Journal entry
    journal_entry = {
        "date": effective_date, "description": f"Retirement of partner: {partner_name}",
        "entries": [
            {"account_code": "3000", "description": "Partner Capital", "debit": capital_balance, "credit": 0},
            {"account_code": "3100", "description": "Partner Current Account", "debit": current_account_balance, "credit": 0},
            {"account_code": "1000", "description": "Cash/Bank", "debit": 0, "credit": total_payable},
        ],
        "reference": f"RET-{partner_id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    change.journal_entry_id = result.get("id")
    return change


@app.post("/changes/admission")
async def record_admission(data: AdmissionDetails):
    """Record new partner admission."""
    data.id = str(uuid.uuid4())
    data.admission_date = data.admission_date or datetime.utcnow()

    journal_entries = []

    # Capital contribution
    if data.capital_contribution > 0:
        journal_entries.append({
            "date": data.admission_date,
            "description": f"Capital contribution by {data.new_partner_name}",
            "entries": [
                {"account_code": "1000", "description": "Cash/Bank", "debit": data.capital_contribution, "credit": 0},
                {"account_code": "3000", "description": f"Partner Capital - {data.new_partner_name}", "debit": 0, "credit": data.capital_contribution},
            ],
            "reference": f"ADM-{data.new_partner_id[:8]}"
        })

    # Goodwill premium
    if data.goodwill_paid > 0:
        journal_entries.append({
            "date": data.admission_date,
            "description": "Goodwill premium",
            "entries": [
                {"account_code": "1000", "description": "Cash/Bank", "debit": data.goodwill_paid, "credit": 0},
                {"account_code": "1500", "description": "Goodwill", "debit": 0, "credit": data.goodwill_paid},
            ],
            "reference": f"GW-{data.new_partner_id[:8]}"
        })

    for entry in journal_entries:
        result = await call_accounting_service("POST", "/journal-entries", entry)
        data.journal_entry_ids.append(result.get("id", ""))

    admissions.append(data)
    return data


@app.post("/changes/death")
async def record_death(
    partnership_id: str, partner_id: str, partner_name: str, effective_date: datetime,
    capital_balance: float, current_account_balance: float, executor_name: str
):
    """Record partner death."""
    total_payable = capital_balance + current_account_balance

    change = PartnerChange(
        partnership_id=partnership_id, change_type=ChangeType.DEATH,
        partner_id=partner_id, partner_name=partner_name, effective_date=effective_date,
        capital_balance=capital_balance, current_account_balance=current_account_balance,
        total_payable=total_payable, notes=f"Payable to executor: {executor_name}"
    )
    changes.append(change)
    return change


@app.get("/changes")
async def list_changes(change_type: Optional[ChangeType] = None, partnership_id: Optional[str] = None):
    """List all partner changes."""
    result = changes
    if change_type:
        result = [c for c in result if c.change_type == change_type]
    if partnership_id:
        result = [c for c in result if c.partnership_id == partnership_id]
    return {"changes": result, "count": len(result)}


@app.post("/changes/{change_id}/settle")
async def settle_change(change_id: str):
    """Mark change as settled."""
    change = next((c for c in changes if c.id == change_id), None)
    if not change:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Change not found")
    change.settlement_status = "settled"
    return change


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)