"""
Vimbai General Reserve Service
Manages general reserve fund operations.
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

SERVICE_NAME = "general-reserve-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8053"))
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

app = FastAPI(title="Vimbai General Reserve Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class GeneralReserve(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    reserve_name: str
    description: str = ""
    current_balance: float = 0
    target_balance: Optional[float] = None
    minimum_balance: float = 0
    funding_source: str = "retained_earnings"  # retained_earnings, share_premium, specific_allocation
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


class ReserveAllocation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reserve_id: str
    amount: float
    allocation_date: datetime
    source: str  # retained_earnings, share_premium, profit
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ReserveUtilization(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reserve_id: str
    amount: float
    utilization_date: datetime
    purpose: str  # asset_purchase, debt_repayment, working_capital, bonus_issue
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


reserves: List[GeneralReserve] = []
allocations: List[ReserveAllocation] = []
utilizations: List[ReserveUtilization] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "General reserve management"}


@app.post("/reserves/create")
async def create_reserve(
    company_id: str,
    reserve_name: str,
    description: str = "",
    initial_balance: float = 0,
    target_balance: Optional[float] = None,
    minimum_balance: float = 0,
    funding_source: str = "retained_earnings",
):
    """Create a general reserve."""
    reserve = GeneralReserve(
        company_id=company_id,
        reserve_name=reserve_name,
        description=description,
        current_balance=initial_balance,
        target_balance=target_balance,
        minimum_balance=minimum_balance,
        funding_source=funding_source,
    )

    if initial_balance > 0:
        journal_entry = {
            "date": datetime.utcnow(),
            "description": f"Creation of {reserve_name} reserve",
            "entries": [
                {"account_code": "3300", "description": "Retained Earnings", "debit": initial_balance, "credit": 0},
                {"account_code": "3310", "description": "General Reserve", "debit": 0, "credit": initial_balance},
            ],
            "reference": f"RESERVE-CREATE-{reserve.id[:8]}",
        }
        result = await call_accounting_service("POST", "/journal-entries", journal_entry)
        reserve.journal_entry_id = result.get("id")

    reserves.append(reserve)
    return reserve


@app.post("/reserves/{reserve_id}/allocate")
async def allocate_to_reserve(
    reserve_id: str, amount: float, source: str, description: str, allocation_date: Optional[datetime] = None
):
    """Allocate funds to reserve."""
    reserve = next((r for r in reserves if r.id == reserve_id), None)
    if not reserve:
        return {"error": "Reserve not found"}

    if allocation_date is None:
        allocation_date = datetime.utcnow()

    allocation = ReserveAllocation(
        reserve_id=reserve_id, amount=amount, allocation_date=allocation_date, source=source, description=description
    )

    reserve.current_balance += amount
    reserve.updated_at = datetime.utcnow()

    source_account = "3300" if source == "retained_earnings" else "3210"
    journal_entry = {
        "date": allocation_date,
        "description": f"Allocation to {reserve.reserve_name}: {description}",
        "entries": [
            {
                "account_code": source_account,
                "description": source.replace("_", " ").title(),
                "debit": amount,
                "credit": 0,
            },
            {"account_code": "3310", "description": "General Reserve", "debit": 0, "credit": amount},
        ],
        "reference": f"RESERVE-ALLOC-{allocation.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    allocation.journal_entry_id = result.get("id")
    allocations.append(allocation)

    return {"reserve": reserve, "allocation": allocation}


@app.post("/reserves/{reserve_id}/utilize")
async def utilize_reserve(
    reserve_id: str, amount: float, purpose: str, description: str, utilization_date: Optional[datetime] = None
):
    """Utilize funds from reserve."""
    reserve = next((r for r in reserves if r.id == reserve_id), None)
    if not reserve:
        return {"error": "Reserve not found"}

    if amount > reserve.current_balance:
        return {"error": "Insufficient reserve balance"}

    if utilization_date is None:
        utilization_date = datetime.utcnow()

    utilization = ReserveUtilization(
        reserve_id=reserve_id,
        amount=amount,
        utilization_date=utilization_date,
        purpose=purpose,
        description=description,
    )

    reserve.current_balance -= amount
    reserve.updated_at = datetime.utcnow()

    dest_account = "1000" if purpose in ["asset_purchase", "working_capital"] else "2310"
    journal_entry = {
        "date": utilization_date,
        "description": f"Utilization of {reserve.reserve_name}: {description}",
        "entries": [
            {"account_code": "3310", "description": "General Reserve", "debit": amount, "credit": 0},
            {
                "account_code": dest_account,
                "description": purpose.replace("_", " ").title(),
                "debit": 0,
                "credit": amount,
            },
        ],
        "reference": f"RESERVE-UTIL-{utilization.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    utilization.journal_entry_id = result.get("id")
    utilizations.append(utilization)

    return {"reserve": reserve, "utilization": utilization}


@app.get("/reserves")
async def list_reserves(company_id: Optional[str] = None):
    """List all reserves."""
    result = reserves
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    return {"reserves": result}


@app.get("/reserves/{reserve_id}")
async def get_reserve(reserve_id: str):
    """Get reserve details."""
    reserve = next((r for r in reserves if r.id == reserve_id), None)
    if not reserve:
        return {"error": "Reserve not found"}
    return reserve


@app.get("/reserves/{reserve_id}/history")
async def get_reserve_history(reserve_id: str):
    """Get reserve allocation and utilization history."""
    reserve_allocations = [a for a in allocations if a.reserve_id == reserve_id]
    reserve_utilizations = [u for u in utilizations if u.reserve_id == reserve_id]
    return {"allocations": reserve_allocations, "utilizations": reserve_utilizations}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
