"""
Vimbai Capital Reconstruction Service
Manages capital reduction and reconstruction schemes.
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

SERVICE_NAME = "capital-reconstruction-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8060"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Capital Reconstruction Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ReconstructionType(str):
    SIMPLIFICATION = "simplification"
    FINANCIAL_RESTRUCTURING = "financial_restructuring"
    WRITE_OFF_EXCESS_CAPITAL = "write_off_excess_capital"
    CONSOLIDATION = "consolidation"
    SUBSTITUTION = "substitution"


class CapitalReconstruction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    reconstruction_type: str
    description: str
    scheme_date: datetime
    court_approval_date: Optional[datetime] = None
    shareholders_approval_date: Optional[datetime] = None
    previous_share_capital: float = 0
    new_share_capital: float = 0
    capital_reduction_amount: float = 0
    share_consolidation_ratio: str = ""  # e.g., "2:1"
    journal_entry_id: Optional[str] = None
    status: str = "draft"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ReconstructionAdjustment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reconstruction_id: str
    account_code: str
    account_name: str
    previous_balance: float
    adjustment_type: str  # write_off, transfer, consolidate
    adjustment_amount: float
    new_balance: float = 0
    description: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ReserveConversion(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    reconstruction_id: str
    from_account: str
    from_account_name: str
    to_account: str
    to_account_name: str
    amount: float
    conversion_date: datetime
    reason: str
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


reconstructions: List[CapitalReconstruction] = []
adjustments: List[ReconstructionAdjustment] = []
reserve_conversions: List[ReserveConversion] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Capital reconstruction management"}


@app.post("/reconstructions/create")
async def create_reconstruction(
    company_id: str, reconstruction_type: str, description: str,
    scheme_date: datetime, previous_share_capital: float, new_share_capital: float,
    share_consolidation_ratio: str = ""
):
    """Create capital reconstruction scheme."""
    reconstruction = CapitalReconstruction(
        company_id=company_id, reconstruction_type=reconstruction_type,
        description=description, scheme_date=scheme_date,
        previous_share_capital=previous_share_capital, new_share_capital=new_share_capital,
        share_consolidation_ratio=share_consolidation_ratio
    )
    reconstruction.capital_reduction_amount = previous_share_capital - new_share_capital
    reconstructions.append(reconstruction)
    return reconstruction


@app.post("/reconstructions/{reconstruction_id}/adjustments/add")
async def add_adjustment(
    reconstruction_id: str, account_code: str, account_name: str,
    previous_balance: float, adjustment_type: str, adjustment_amount: float,
    description: str
):
    """Add reconstruction adjustment."""
    reconstruction = next((r for r in reconstructions if r.id == reconstruction_id), None)
    if not reconstruction:
        return {"error": "Reconstruction not found"}

    adjustment = ReconstructionAdjustment(
        reconstruction_id=reconstruction_id, account_code=account_code,
        account_name=account_name, previous_balance=previous_balance,
        adjustment_type=adjustment_type, adjustment_amount=adjustment_amount,
        description=description
    )
    adjustment.new_balance = previous_balance + adjustment_amount if adjustment_type == "transfer" else 0
    adjustments.append(adjustment)

    return adjustment


@app.post("/reconstructions/{reconstruction_id}/reserve-conversions/add")
async def add_reserve_conversion(
    reconstruction_id: str, from_account: str, from_account_name: str,
    to_account: str, to_account_name: str, amount: float, reason: str,
    conversion_date: Optional[datetime] = None
):
    """Add reserve conversion entry."""
    if conversion_date is None:
        conversion_date = datetime.utcnow()

    conversion = ReserveConversion(
        reconstruction_id=reconstruction_id, from_account=from_account,
        from_account_name=from_account_name, to_account=to_account,
        to_account_name=to_account_name, amount=amount, conversion_date=conversion_date,
        reason=reason
    )

    journal_entry = {
        "date": conversion_date,
        "description": f"Reserve conversion: {from_account_name} to {to_account_name}",
        "entries": [
            {"account_code": from_account, "description": from_account_name, "debit": amount, "credit": 0},
            {"account_code": to_account, "description": to_account_name, "debit": 0, "credit": amount},
        ],
        "reference": f"RCONV-{conversion.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    conversion.journal_entry_id = result.get("id")
    reserve_conversions.append(conversion)

    return conversion


@app.post("/reconstructions/{reconstruction_id}/approve")
async def approve_reconstruction(
    reconstruction_id: str, court_approval_date: Optional[datetime] = None,
    shareholders_approval_date: Optional[datetime] = None
):
    """Approve and execute reconstruction."""
    reconstruction = next((r for r in reconstructions if r.id == reconstruction_id), None)
    if not reconstruction:
        return {"error": "Reconstruction not found"}

    if court_approval_date:
        reconstruction.court_approval_date = court_approval_date
    if shareholders_approval_date:
        reconstruction.shareholders_approval_date = shareholders_approval_date

    reconstruction.status = "approved"

    # Get all adjustments and conversions for this reconstruction
    reconstruction_adjustments = [a for a in adjustments if a.reconstruction_id == reconstruction_id]
    reconstruction_conversions = [c for c in reserve_conversions if c.reconstruction_id == reconstruction_id]

    # Create main reconstruction journal entry
    entries = [
        # Reduce share capital
        {"account_code": "3200", "description": "Share Capital", "debit": reconstruction.capital_reduction_amount, "credit": 0},
    ]

    # Add write-offs from adjustments
    for adj in reconstruction_adjustments:
        if adj.adjustment_type == "write_off":
            entries.append({"account_code": adj.account_code, "description": adj.account_name, "debit": adj.adjustment_amount, "credit": 0})

    # Credits go to various reserves
    total_credits = reconstruction.capital_reduction_amount + sum(
        adj.adjustment_amount for adj in reconstruction_adjustments if adj.adjustment_type == "write_off"
    )
    entries.append({"account_code": "3300", "description": "Retained Earnings / P&L", "debit": 0, "credit": total_credits})

    journal_entry = {
        "date": reconstruction.scheme_date,
        "description": f"Capital reconstruction: {reconstruction.description}",
        "entries": entries,
        "reference": f"RECONST-{reconstruction.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    reconstruction.journal_entry_id = result.get("id")
    reconstruction.status = "completed"

    return {
        "reconstruction": reconstruction,
        "adjustments": reconstruction_adjustments,
        "conversions": reconstruction_conversions
    }


@app.get("/reconstructions")
async def list_reconstructions(company_id: Optional[str] = None, status: Optional[str] = None):
    """List capital reconstructions."""
    result = reconstructions
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    if status:
        result = [r for r in result if r.status == status]
    return {"reconstructions": result}


@app.get("/reconstructions/{reconstruction_id}")
async def get_reconstruction(reconstruction_id: str):
    """Get reconstruction details with adjustments."""
    reconstruction = next((r for r in reconstructions if r.id == reconstruction_id), None)
    if not reconstruction:
        return {"error": "Reconstruction not found"}

    reconstruction_adjustments = [a for a in adjustments if a.reconstruction_id == reconstruction_id]
    reconstruction_conversions = [c for c in reserve_conversions if c.reconstruction_id == reconstruction_id]

    return {
        "reconstruction": reconstruction,
        "adjustments": reconstruction_adjustments,
        "reserve_conversions": reconstruction_conversions
    }


@app.get("/summary/{company_id}")
async def get_reconstruction_summary(company_id: str):
    """Get capital reconstruction summary."""
    company_reconstructions = [r for r in reconstructions if r.company_id == company_id]

    total_reduction = sum(r.capital_reduction_amount for r in company_reconstructions)
    completed = len([r for r in company_reconstructions if r.status == "completed"])

    return {
        "company_id": company_id,
        "total_reconstructions": len(company_reconstructions),
        "completed_reconstructions": completed,
        "total_capital_reduced": total_reduction,
        "reconstructions": company_reconstructions
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)