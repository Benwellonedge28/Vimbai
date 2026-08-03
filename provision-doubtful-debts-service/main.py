"""
Vimbai Provision for Doubtful Debts Service
Manages specific and general provisions for doubtful debts.
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

SERVICE_NAME = "provision-doubtful-debts-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8034"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Provision for Doubtful Debts Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ProvisionType(str, Enum):
    GENERAL = "general"
    SPECIFIC = "specific"
    FLAT_RATE = "flat_rate"


class DoubtfulDebtEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    debtor_id: str
    debtor_name: str
    invoice_number: str
    invoice_date: datetime
    due_date: datetime
    amount: float
    outstanding: float
    days_overdue: int = 0
    risk_assessment: str = "medium"  # low, medium, high, doubtful
    provision_percent: float = 0
    provision_amount: float = 0
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class DoubtfulDebtProvision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    period_end: datetime
    provision_type: ProvisionType
    total_debtors: float = 0
    provision_required: float = 0
    provision_opening: float = 0
    provision_adjustment: float = 0
    provision_closing: float = 0
    entries: List[DoubtfulDebtEntry] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


# In-memory storage
doubtful_debt_entries: List[DoubtfulDebtEntry] = []
provision_records: List[DoubtfulDebtProvision] = []
risk_policies: Dict[str, float] = {"low": 1, "medium": 5, "high": 25, "doubtful": 75}


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.error("error", error=str(e))
        return {}


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(f"{AUDIT_SERVICE_URL}/audit", json={
                "action": action, "resource_type": resource_type, "resource_id": resource_id,
                "details": details, "timestamp": datetime.utcnow().isoformat()
            })
    except Exception:
        pass


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Provision for doubtful debts management"}


@app.post("/debtors/add")
async def add_debtor(
    debtor_id: str, debtor_name: str, invoice_number: str, invoice_date: datetime,
    due_date: datetime, amount: float, risk_assessment: str = "medium"
):
    """Add a debtor for doubtful debt provision tracking."""
    entry = DoubtfulDebtEntry(
        debtor_id=debtor_id, debtor_name=debtor_name, invoice_number=invoice_number,
        invoice_date=invoice_date, due_date=due_date, amount=amount, outstanding=amount,
        risk_assessment=risk_assessment, provision_percent=risk_policies.get(risk_assessment, 5),
    )
    entry.provision_amount = entry.outstanding * (entry.provision_percent / 100)
    doubtful_debt_entries.append(entry)
    await call_audit_service("CREATE", "debtor", debtor_id, {"name": debtor_name})
    return entry


@app.post("/debtors/{debtor_id}/assess")
async def assess_debtor_risk(debtor_id: str, risk_assessment: str, notes: Optional[str] = None):
    """Update debtor risk assessment."""
    entry = next((e for e in doubtful_debt_entries if e.debtor_id == debtor_id), None)
    if not entry:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Debtor not found")

    entry.risk_assessment = risk_assessment
    entry.provision_percent = risk_policies.get(risk_assessment, 5)
    entry.provision_amount = entry.outstanding * (entry.provision_percent / 100)
    if notes:
        entry.notes = notes

    await call_audit_service("ASSESS", "debtor", debtor_id, {"risk": risk_assessment})
    return entry


@app.post("/calculate-provision")
async def calculate_provision(period_end: datetime, provision_type: ProvisionType = ProvisionType.SPECIFIC):
    """Calculate total provision required."""
    entries = []
    total_provision = 0

    for entry in doubtful_debt_entries:
        days = (period_end - entry.due_date).days
        entry.days_overdue = max(0, days)

        if entry.days_overdue >= 180:
            entry.risk_assessment = "doubtful"
            entry.provision_percent = 75
        elif entry.days_overdue >= 90:
            entry.risk_assessment = "high"
            entry.provision_percent = 25
        elif entry.days_overdue >= 60:
            entry.risk_assessment = "medium"
            entry.provision_percent = 5

        entry.provision_amount = entry.outstanding * (entry.provision_percent / 100)
        total_provision += entry.provision_amount
        entries.append(entry)

    return {
        "period_end": period_end,
        "provision_type": provision_type,
        "total_debtors": len(entries),
        "total_outstanding": sum(e.outstanding for e in entries),
        "total_provision_required": total_provision,
        "entries": [
            {"debtor_id": e.debtor_id, "name": e.debtor_name, "outstanding": e.outstanding,
             "risk": e.risk_assessment, "provision_percent": e.provision_percent, "provision_amount": e.provision_amount}
            for e in entries
        ]
    }


@app.post("/provision/create")
async def create_provision(period_end: datetime, provision_opening: float = 0, provision_type: ProvisionType = ProvisionType.SPECIFIC):
    """Create provision journal entry."""
    calc = await calculate_provision(period_end, provision_type)
    provision_required = calc["total_provision_required"]
    provision_adjustment = provision_required - provision_opening

    journal_entry = {
        "date": period_end,
        "description": f"Provision for doubtful debts - {period_end.date()}",
        "entries": [
            {"account_code": "6300", "description": "Bad Debts Expense", "debit": abs(provision_adjustment) if provision_adjustment > 0 else 0, "credit": 0},
            {"account_code": "1310", "description": "Provision for Doubtful Debts", "debit": 0, "credit": abs(provision_adjustment) if provision_adjustment > 0 else provision_adjustment},
        ],
        "reference": f"DDBT-PROV-{period_end.strftime('%Y%m')}",
    }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    provision = DoubtfulDebtProvision(
        period_end=period_end, provision_type=provision_type,
        total_debtors=calc["total_debtors"], provision_required=provision_required,
        provision_opening=provision_opening, provision_adjustment=provision_adjustment,
        provision_closing=provision_required, entries=calc["entries"], journal_entry_id=result.get("id")
    )
    provision_records.append(provision)

    await call_audit_service("CREATE", "provision", provision.id, {"amount": provision_required})
    return provision


@app.get("/provisions")
async def list_provisions(limit: int = 12):
    """Get provision history."""
    return {"provisions": provision_records[-limit:]}


@app.get("/debtors")
async def list_debtors(risk_assessment: Optional[str] = None):
    """List all debtors."""
    result = doubtful_debt_entries
    if risk_assessment:
        result = [e for e in result if e.risk_assessment == risk_assessment]
    return {"debtors": result, "count": len(result)}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)