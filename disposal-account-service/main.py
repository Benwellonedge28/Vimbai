"""
FinAcc Disposal Account Service
Manages disposal of fixed assets and calculation of profit/loss on disposal.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "disposal-account-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8036"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Disposal Account Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class DisposalReason(str):
    SOLD = "sold"
    SCRAPPED = "scrapped"
    STOLEN = "stolen"
    DESTROYED = "destroyed"
    DONATED = "donated"
    RETURNED = "returned_to_lease"


class DisposalEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    asset_name: str
    asset_code: str
    disposal_date: datetime
    reason: DisposalReason
    original_cost: float
    accumulated_depreciation: float
    net_book_value: float
    disposal_proceeds: float
    profit_on_disposal: float = 0
    loss_on_disposal: float = 0
    journal_entry_id: Optional[str] = None
    status: str = "pending"
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Fixed asset disposal management"}


@app.post("/disposal/calculate")
async def calculate_disposal(
    asset_id: str, asset_name: str, asset_code: str, original_cost: float,
    accumulated_depreciation: float, disposal_proceeds: float, disposal_date: datetime, reason: DisposalReason
):
    """Calculate profit/loss on disposal."""
    net_book_value = original_cost - accumulated_depreciation
    disposal_proceeds = disposal_proceeds

    profit = 0
    loss = 0
    if disposal_proceeds > net_book_value:
        profit = disposal_proceeds - net_book_value
    else:
        loss = net_book_value - disposal_proceeds

    entry = DisposalEntry(
        asset_id=asset_id, asset_name=asset_name, asset_code=asset_code,
        disposal_date=disposal_date, reason=reason, original_cost=original_cost,
        accumulated_depreciation=accumulated_depreciation, net_book_value=net_book_value,
        disposal_proceeds=disposal_proceeds, profit_on_disposal=profit, loss_on_disposal=loss
    )

    await call_audit_service("CALCULATE", "disposal", asset_id, {"profit": profit, "loss": loss})
    return entry


@app.post("/disposal/post")
async def post_disposal(entry: DisposalEntry):
    """Post disposal journal entry."""
    entries = [
        {"account_code": "1000", "description": "Cash/Bank", "debit": entry.disposal_proceeds, "credit": 0},
        {"account_code": "1520", "description": "Accumulated Depreciation", "debit": entry.accumulated_depreciation, "credit": 0},
    ]

    if entry.profit_on_disposal > 0:
        entries.extend([
            {"account_code": "1500", "description": f"Asset Cost - {entry.asset_name}", "debit": 0, "credit": entry.original_cost},
            {"account_code": "7100", "description": "Profit on Disposal", "debit": 0, "credit": entry.profit_on_disposal},
        ])
    else:
        entries.extend([
            {"account_code": "1500", "description": f"Asset Cost - {entry.asset_name}", "debit": 0, "credit": entry.original_cost},
            {"account_code": "7200", "description": "Loss on Disposal", "debit": entry.loss_on_disposal, "credit": 0},
        ])

    journal_entry = {
        "date": entry.disposal_date,
        "description": f"Disposal of {entry.asset_name} - {entry.reason}",
        "entries": entries,
        "reference": f"DISP-{entry.asset_id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    entry.journal_entry_id = result.get("id")
    entry.status = "posted"

    await call_audit_service("POST", "disposal", entry.asset_id, {"journal_id": result.get("id")})
    return entry


@app.get("/disposals")
async def list_disposals(status: Optional[str] = None):
    """List all disposals."""
    return {"disposals": []}


@app.get("/summary")
async def get_disposal_summary():
    """Get disposal summary."""
    return {"total_disposals": 0, "total_profit": 0, "total_loss": 0}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)