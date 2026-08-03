"""
Vimbai Provision for Bad Debts Service
Manages provisions for bad and doubtful debts.
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

SERVICE_NAME = "provision-bad-debts-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8032"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Provision for Bad Debts Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ProvisionMethod(str, Enum):
    PERCENTAGE_OF_SALES = "percentage_of_sales"
    PERCENTAGE_OF_DEBTORS = "percentage_of_debtors"
    AGEING_ANALYSIS = "ageing_analysis"
    SPECIFIC_DEBTOR_PROVISION = "specific_debtor_provision"


class DebtorStatus(str, Enum):
    CURRENT = "current"
    OVERDUE_30 = "overdue_30"
    OVERDUE_60 = "overdue_60"
    OVERDUE_90 = "overdue_90"
    OVERDUE_180 = "overdue_180"
    DOUBTFUL = "doubtful"
    BAD = "bad"


class ProvisionEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    debtor_id: str
    debtor_name: str
    invoice_number: str
    original_amount: float
    outstanding_amount: float
    days_outstanding: int = 0
    status: DebtorStatus
    provision_percent: float = 0
    provision_amount: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ProvisionPolicy(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    method: ProvisionMethod
    policy_rules: Dict[str, float] = {}  # days -> percentage
    default_provision_percent: float = 0
    is_active: bool = True
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ProvisionJournal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    period_end: datetime
    total_debtors: float = 0
    provision_opening: float = 0
    provision_required: float = 0
    provision_adjustment: float = 0
    provision_closing: float = 0
    entries: List[ProvisionEntry] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


# In-memory storage
provision_policies: Dict[str, ProvisionPolicy] = {}
provision_journals: List[ProvisionJournal] = {}
debtor_records: List[ProvisionEntry] = []


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
        logger.error("accounting_service_error", error=str(e))
        return {}


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(f"{AUDIT_SERVICE_URL}/audit", json={
                "action": action, "resource_type": resource_type, "resource_id": resource_id,
                "details": details, "timestamp": datetime.utcnow().isoformat()
            })
    except Exception as e:
        logger.error("audit_error", error=str(e))


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Provision for bad and doubtful debts management"}


@app.post("/policies", response_model=ProvisionPolicy, status_code=status.HTTP_201_CREATED)
async def create_policy(data: ProvisionPolicy):
    """Create a provision policy."""
    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    provision_policies[data.id] = data
    await call_audit_service("CREATE", "provision_policy", data.id, {"name": data.name, "method": data.method})
    return data


@app.get("/policies")
async def list_policies(is_active: Optional[bool] = None):
    """List provision policies."""
    result = list(provision_policies.values())
    if is_active is not None:
        result = [p for p in result if p.is_active == is_active]
    return {"policies": result}


@app.post("/debtors/add")
async def add_debtor(debtor_id: str, debtor_name: str, invoice_number: str, original_amount: float, invoice_date: datetime):
    """Add a debtor for tracking."""
    entry = ProvisionEntry(
        debtor_id=debtor_id, debtor_name=debtor_name, invoice_number=invoice_number,
        original_amount=original_amount, outstanding_amount=original_amount,
        days_outstanding=0, status=DebtorStatus.CURRENT
    )
    debtor_records.append(entry)
    await call_audit_service("CREATE", "debtor", debtor_id, {"name": debtor_name})
    return entry


@app.post("/calculate-provision")
async def calculate_provision(period_end: datetime, policy_id: Optional[str] = None, total_debtors: float = 0):
    """Calculate provision for bad debts."""
    policy = provision_policies.get(policy_id) if policy_id else None

    # Default aging-based percentages
    if not policy:
        policy_rules = {
            "0": 0, "30": 1, "60": 5, "90": 10, "180": 50, "365": 100
        }
    else:
        policy_rules = policy.policy_rules

    # Calculate provision based on aging
    provision_required = 0
    entries = []

    for debtor in debtor_records:
        days = (period_end - datetime.utcnow()).days + debtor.days_outstanding
        debtor.days_outstanding = max(0, days)

        if debtor.days_outstanding >= 365:
            debtor.status = DebtorStatus.BAD
            debtor.provision_percent = 100
        elif debtor.days_outstanding >= 180:
            debtor.status = DebtorStatus.OVERDUE_180
            debtor.provision_percent = policy_rules.get("180", 50)
        elif debtor.days_outstanding >= 90:
            debtor.status = DebtorStatus.OVERDUE_90
            debtor.provision_percent = policy_rules.get("90", 10)
        elif debtor.days_outstanding >= 60:
            debtor.status = DebtorStatus.OVERDUE_60
            debtor.provision_percent = policy_rules.get("60", 5)
        elif debtor.days_outstanding >= 30:
            debtor.status = DebtorStatus.OVERDUE_30
            debtor.provision_percent = policy_rules.get("30", 1)
        else:
            debtor.status = DebtorStatus.CURRENT
            debtor.provision_percent = 0

        debtor.provision_amount = debtor.outstanding_amount * (debtor.provision_percent / 100)
        provision_required += debtor.provision_amount
        entries.append(debtor)

    return {
        "total_debtors": total_debtors or sum(d.outstanding_amount for d in debtor_records),
        "provision_required": provision_required,
        "entries": entries,
        "ageing_summary": {
            "current": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.CURRENT),
            "overdue_30": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_30),
            "overdue_60": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_60),
            "overdue_90": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_90),
            "overdue_180": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.OVERDUE_180),
            "bad": sum(e.outstanding_amount for e in entries if e.status == DebtorStatus.BAD),
        }
    }


@app.post("/journal/create")
async def create_provision_journal(period_end: datetime, total_debtors: float, provision_opening: float = 0):
    """Create journal entry for provision."""
    calc = await calculate_provision(period_end, total_debtors=total_debtors)
    provision_required = calc["provision_required"]
    provision_adjustment = provision_required - provision_opening

    journal_entry = {
        "date": period_end,
        "description": f"Provision for doubtful debts as at {period_end.date()}",
        "entries": [
            {"account_code": "6300", "description": "Bad Debts Expense", "debit": provision_adjustment if provision_adjustment > 0 else 0, "credit": 0},
            {"account_code": "1310", "description": "Provision for Doubtful Debts", "debit": 0, "credit": provision_adjustment if provision_adjustment > 0 else abs(provision_adjustment)},
        ],
        "reference": f"PROV-{period_end.strftime('%Y%m')}",
    }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    journal = ProvisionJournal(
        period_end=period_end, total_debtors=total_debtors,
        provision_opening=provision_opening, provision_required=provision_required,
        provision_adjustment=provision_adjustment, provision_closing=provision_required,
        entries=calc["entries"], journal_entry_id=result.get("id")
    )
    provision_journals.append(journal)

    await call_audit_service("CREATE", "provision_journal", journal.id, {"amount": provision_required})
    return journal


@app.get("/provision-history")
async def get_provision_history(limit: int = 12):
    """Get provision history."""
    return {"journals": provision_journals[-limit:]}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)