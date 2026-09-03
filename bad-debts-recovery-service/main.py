"""
Vimbai Bad Debts Recovery Service
Tracks and records recovery of previously written-off bad debts.
"""

import os
import uuid
from enum import Enum
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "bad-debts-recovery-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8033"))
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

app = FastAPI(title="Vimbai Bad Debts Recovery Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class RecoveryStatus(str, Enum):
    PENDING = "pending"
    PARTIAL = "partial"
    COMPLETED = "completed"
    UNRECOVERABLE = "unrecoverable"


class RecoveryEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    original_debt_id: str
    debtor_name: str
    original_amount: float
    written_off_amount: float
    recovered_amount: float = 0
    recovery_date: Optional[datetime] = None
    status: RecoveryStatus = RecoveryStatus.PENDING
    payment_method: Optional[str] = None
    reference: Optional[str] = None
    notes: Optional[str] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class RecoveryJournal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    recovery_id: str
    amount: float
    date: datetime
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


# In-memory storage
bad_debts_records: List[RecoveryEntry] = []
recovery_journals: List[RecoveryJournal] = []


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
            await client.post(
                f"{AUDIT_SERVICE_URL}/audit",
                json={
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )
    except Exception as e:
        logger.error("audit_error", error=str(e))


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Bad debts recovery tracking"}


@app.post("/bad-debts/register")
async def register_bad_debt(
    debt_id: str,
    debtor_name: str,
    original_amount: float,
    written_off_amount: float,
    written_off_date: datetime,
    notes: Optional[str] = None,
):
    """Register a bad debt for recovery tracking."""
    entry = RecoveryEntry(
        original_debt_id=debt_id,
        debtor_name=debtor_name,
        original_amount=original_amount,
        written_off_amount=written_off_amount,
        notes=notes,
    )
    bad_debts_records.append(entry)
    await call_audit_service("CREATE", "bad_debt", debt_id, {"amount": written_off_amount})
    return entry


@app.post("/bad-debts/{debt_id}/recover")
async def record_recovery(
    debt_id: str,
    amount: float,
    recovery_date: Optional[datetime] = None,
    payment_method: Optional[str] = None,
    reference: Optional[str] = None,
):
    """Record recovery of a bad debt."""
    entry = next((e for e in bad_debts_records if e.original_debt_id == debt_id), None)
    if not entry:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bad debt record not found")

    if entry.status == RecoveryStatus.COMPLETED:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Recovery already completed")

    recovery_date = recovery_date or datetime.utcnow()

    # Create journal entry
    journal_entry = {
        "date": recovery_date,
        "description": f"Recovery of bad debt from {entry.debtor_name}",
        "entries": [
            {"account_code": "1000", "description": "Cash/Bank", "debit": amount, "credit": 0},
            {
                "account_code": "1310",
                "description": "Provision for Doubtful Debts",
                "debit": amount if entry.recovered_amount == 0 else 0,
                "credit": 0,
            },
            {"account_code": "6400", "description": "Bad Debts Recovered", "debit": 0, "credit": amount},
        ],
        "reference": f"BAD-DBT-REC-{debt_id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)

    # Record the recovery
    recovery = RecoveryJournal(
        recovery_id=debt_id,
        amount=amount,
        date=recovery_date,
        description=f"Recovery from {entry.debtor_name}",
        journal_entry_id=result.get("id"),
    )
    recovery_journals.append(recovery)

    entry.recovered_amount += amount
    entry.recovery_date = recovery_date
    entry.payment_method = payment_method
    entry.reference = reference

    if entry.recovered_amount >= entry.written_off_amount:
        entry.status = RecoveryStatus.COMPLETED
    else:
        entry.status = RecoveryStatus.PARTIAL

    await call_audit_service(
        "RECOVER", "bad_debt", debt_id, {"amount": amount, "total_recovered": entry.recovered_amount}
    )
    return {"entry": entry, "recovery": recovery}


@app.get("/bad-debts")
async def list_bad_debts(status: Optional[RecoveryStatus] = None):
    """List all bad debt records."""
    result = bad_debts_records
    if status:
        result = [e for e in result if e.status == status]
    return {"records": result, "total": len(result)}


@app.get("/bad-debts/{debt_id}")
async def get_bad_debt(debt_id: str):
    """Get bad debt details."""
    entry = next((e for e in bad_debts_records if e.original_debt_id == debt_id), None)
    if not entry:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bad debt record not found")
    return entry


@app.get("/recovery-summary")
async def get_recovery_summary():
    """Get recovery summary statistics."""
    total_bad_debts = len(bad_debts_records)
    total_written_off = sum(e.written_off_amount for e in bad_debts_records)
    total_recovered = sum(e.recovered_amount for e in bad_debts_records)
    pending = sum(
        e.written_off_amount - e.recovered_amount for e in bad_debts_records if e.status != RecoveryStatus.COMPLETED
    )

    return {
        "total_bad_debts": total_bad_debts,
        "total_written_off": total_written_off,
        "total_recovered": total_recovered,
        "pending_recovery": pending,
        "recovery_rate": (total_recovered / total_written_off * 100) if total_written_off > 0 else 0,
        "status_breakdown": {
            "pending": len([e for e in bad_debts_records if e.status == RecoveryStatus.PENDING]),
            "partial": len([e for e in bad_debts_records if e.status == RecoveryStatus.PARTIAL]),
            "completed": len([e for e in bad_debts_records if e.status == RecoveryStatus.COMPLETED]),
        },
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
