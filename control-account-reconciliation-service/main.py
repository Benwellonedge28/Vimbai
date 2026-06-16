"""
FinAcc Control Account Reconciliation Service
Handles reconciliation of all control accounts and error handling.
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

SERVICE_NAME = "control-account-reconciliation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8040"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Control Account Reconciliation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ControlAccountType(str, Enum):
    SALES_LEDGER = "sales_ledger"
    PURCHASES_LEDGER = "purchases_ledger"
    BILLS_RECEIVABLE = "bills_receivable"
    BILLS_PAYABLE = "bills_payable"


class ErrorType(str, Enum):
    DUPLICATE_ENTRY = "duplicate_entry"
    POSTING_ERROR = "posting_error"
    MISSING_ENTRY = "missing_entry"
    AMOUNT_MISMATCH = "amount_mismatch"
    TIMING_DIFFERENCE = "timing_difference"
    TRANSFER_ERROR = "transfer_error"
    ROUNDING_ERROR = "rounding_error"


class ReconciliationError(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    error_type: ErrorType
    control_account_type: ControlAccountType
    transaction_id: Optional[str] = None
    description: str
    amount: float = 0
    expected_amount: float = 0
    variance: float = 0
    status: str = "pending"  # pending, resolved, written_off
    resolution: Optional[str] = None
    resolved_by: Optional[str] = None
    resolved_at: Optional[datetime] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ReconciliationReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    as_of_date: datetime
    control_account_type: ControlAccountType
    control_balance: float = 0
    ledger_total: float = 0
    difference: float = 0
    error_count: int = 0
    errors: List[ReconciliationError] = []
    status: str = "pending"
    created_at: datetime = Field(default_factory=datetime.utcnow)


# In-memory storage
reconciliation_errors: List[ReconciliationError] = []
reconciliation_reports: List[ReconciliationReport] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Control account reconciliation and error handling"}


@app.post("/errors/report")
async def report_error(data: ReconciliationError):
    """Report a reconciliation error."""
    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    data.variance = abs(data.amount - data.expected_amount)
    reconciliation_errors.append(data)
    await call_audit_service("CREATE", "error", data.id, {"type": data.error_type, "amount": data.variance})
    return data


@app.get("/errors")
async def list_errors(status: Optional[str] = None, error_type: Optional[ErrorType] = None):
    """List all reconciliation errors."""
    result = reconciliation_errors
    if status:
        result = [e for e in result if e.status == status]
    if error_type:
        result = [e for e in result if e.error_type == error_type]
    return {"errors": result, "count": len(result)}


@app.post("/errors/{error_id}/resolve")
async def resolve_error(error_id: str, resolution: str, resolved_by: str):
    """Resolve a reconciliation error."""
    error = next((e for e in reconciliation_errors if e.id == error_id), None)
    if not error:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Error not found")

    error.status = "resolved"
    error.resolution = resolution
    error.resolved_by = resolved_by
    error.resolved_at = datetime.utcnow()

    await call_audit_service("RESOLVE", "error", error_id, {"resolution": resolution})
    return error


@app.post("/reconcile/{account_type}")
async def reconcile_control_account(account_type: ControlAccountType, as_of_date: datetime, control_balance: float, ledger_total: float):
    """Perform reconciliation for a control account."""
    difference = control_balance - ledger_total
    errors = [e for e in reconciliation_errors if e.control_account_type == account_type and e.status == "pending"]

    report = ReconciliationReport(
        as_of_date=as_of_date, control_account_type=account_type,
        control_balance=control_balance, ledger_total=ledger_total,
        difference=difference, error_count=len(errors), errors=errors,
        status="reconciled" if abs(difference) < 0.01 else "unreconciled"
    )
    reconciliation_reports.append(report)

    await call_audit_service("RECONCILE", "report", report.id, {"difference": difference})
    return report


@app.get("/reports")
async def list_reports(limit: int = 10):
    """List reconciliation reports."""
    return {"reports": reconciliation_reports[-limit:]}


@app.get("/summary")
async def get_reconciliation_summary():
    """Get summary of all reconciliation issues."""
    return {
        "total_errors": len(reconciliation_errors),
        "pending_errors": len([e for e in reconciliation_errors if e.status == "pending"]),
        "resolved_errors": len([e for e in reconciliation_errors if e.status == "resolved"]),
        "total_variance": sum(e.variance for e in reconciliation_errors if e.status == "pending"),
        "by_type": {
            et.value: len([e for e in reconciliation_errors if e.error_type == et])
            for et in ErrorType
        }
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)