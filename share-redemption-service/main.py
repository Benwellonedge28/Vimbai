"""
Vimbai Share Redemption Service
Manages redemption of shares including legal requirements and procedures.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "share-redemption-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8059"))
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

app = FastAPI(title="Vimbai Share Redemption Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class RedemptionMethod(str, Enum):
    OUT_OF_PROCEEDS = "proceeds"
    OUT_OF_FRRESH_ISSUE = "fresh_issue"
    OUT_OF_EXISTING_ASSETS = "existing_assets"
    COMBINATION = "combination"


class ShareRedemption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    share_class: str  # preference, ordinary
    shares_redeemed: int
    nominal_value: float
    redemption_price: float
    total_redemption_value: float = 0
    redemption_date: datetime
    redemption_method: str  # proceeds, fresh_issue, existing_assets, combination
    authority_date: datetime  # When redemption was authorized
    statutory_declaration_date: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    status: str = "pending"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class FreshIssueForRedemption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    redemption_id: str
    shares_issued: int
    issue_price: float
    nominal_value: float
    total_proceeds: float = 0
    issue_date: datetime
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CRRRequirement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    redemption_id: str
    nominal_value_of_shares: float
    proceeds_used: float
    fresh_issue_proceeds: float
    minimum_crr_required: float = 0
    crr_created: float = 0
    source_of_crr: str
    compliance_status: str = "pending"
    created_at: datetime = Field(default_factory=datetime.utcnow)


redemptions: List[ShareRedemption] = []
fresh_issues: List[FreshIssueForRedemption] = []
crr_requirements: List[CRRRequirement] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Share redemption management"}


@app.post("/redemptions/initiate")
async def initiate_redemption(
    company_id: str,
    share_class: str,
    shares_redeemed: int,
    nominal_value: float,
    redemption_price: float,
    redemption_date: datetime,
    redemption_method: str,
    authority_date: datetime,
):
    """Initiate share redemption."""
    redemption = ShareRedemption(
        company_id=company_id,
        share_class=share_class,
        shares_redeemed=shares_redeemed,
        nominal_value=nominal_value,
        redemption_price=redemption_price,
        redemption_date=redemption_date,
        redemption_method=redemption_method,
        authority_date=authority_date,
    )
    redemption.total_redemption_value = shares_redeemed * redemption_price

    # For redemption out of proceeds, CRR must equal the nominal value of shares redeemed
    # less the proceeds of a fresh issue
    if redemption_method == "proceeds":
        crr_req = CRRRequirement(
            redemption_id=redemption.id,
            nominal_value_of_shares=shares_redeemed * nominal_value,
            proceeds_used=redemption.total_redemption_value,
            fresh_issue_proceeds=0,
            minimum_crr_required=shares_redeemed * nominal_value,
            crr_created=0,
            source_of_crr="requires_calculation",
        )
        crr_requirements.append(crr_req)
        redemption.status = "awaiting_crr"

    redemptions.append(redemption)
    return redemption


@app.post("/redemptions/{redemption_id}/fresh-issue")
async def record_fresh_issue(
    redemption_id: str, shares_issued: int, issue_price: float, nominal_value: float, issue_date: datetime
):
    """Record fresh issue to fund redemption."""
    redemption = next((r for r in redemptions if r.id == redemption_id), None)
    if not redemption:
        return {"error": "Redemption not found"}

    fresh_issue = FreshIssueForRedemption(
        redemption_id=redemption_id,
        shares_issued=shares_issued,
        issue_price=issue_price,
        nominal_value=nominal_value,
        issue_date=issue_date,
    )
    fresh_issue.total_proceeds = shares_issued * issue_price

    # Record fresh issue journal entry
    journal_entry = {
        "date": issue_date,
        "description": f"Fresh issue to fund redemption",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": fresh_issue.total_proceeds, "credit": 0},
            {
                "account_code": "3200",
                "description": "Share Capital",
                "debit": 0,
                "credit": shares_issued * nominal_value,
            },
            {
                "account_code": "3210",
                "description": "Share Premium",
                "debit": 0,
                "credit": fresh_issue.total_proceeds - (shares_issued * nominal_value),
            },
        ],
        "reference": f"FRESH-ISS-{fresh_issue.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    fresh_issue.journal_entry_id = result.get("id")
    fresh_issues.append(fresh_issue)

    return fresh_issue


@app.post("/redemptions/{redemption_id}/complete")
async def complete_redemption(redemption_id: str, statutory_declaration_date: Optional[datetime] = None):
    """Complete share redemption after statutory requirements."""
    redemption = next((r for r in redemptions if r.id == redemption_id), None)
    if not redemption:
        return {"error": "Redemption not found"}

    if statutory_declaration_date is None:
        statutory_declaration_date = datetime.utcnow()

    redemption.statutory_declaration_date = statutory_declaration_date

    # Determine CRR amount based on method
    crr_amount = 0
    if redemption.redemption_method == "proceeds":
        # CRR must be created for nominal value of shares
        crr_amount = redemption.shares_redeemed * redemption.nominal_value
    elif redemption.redemption_method == "fresh_issue":
        # No CRR required if fully funded by fresh issue
        crr_amount = 0
    elif redemption.redemption_method == "existing_assets":
        # CRR = nominal value of shares redeemed
        crr_amount = redemption.shares_redeemed * redemption.nominal_value
    else:  # combination
        crr_amount = redemption.shares_redeemed * redemption.nominal_value

    # Create journal entry for redemption
    capital_account = "3205" if redemption.share_class == "preference" else "3200"

    entries = [
        {
            "account_code": capital_account,
            "description": "Share Capital",
            "debit": redemption.shares_redeemed * redemption.nominal_value,
            "credit": 0,
        },
    ]

    if crr_amount > 0:
        entries.append(
            {"account_code": "3220", "description": "Capital Redemption Reserve", "debit": crr_amount, "credit": 0}
        )

    entries.append(
        {"account_code": "1000", "description": "Bank", "debit": 0, "credit": redemption.total_redemption_value}
    )

    journal_entry = {
        "date": redemption.redemption_date,
        "description": f"Redemption of {redemption.shares_redeemed} {redemption.share_class} shares",
        "entries": entries,
        "reference": f"SH-RED-{redemption.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    redemption.journal_entry_id = result.get("id")
    redemption.status = "completed"

    return {"redemption": redemption, "crr_created": crr_amount}


@app.get("/redemptions")
async def list_redemptions(company_id: Optional[str] = None, status: Optional[str] = None):
    """List share redemptions."""
    result = redemptions
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    if status:
        result = [r for r in result if r.status == status]
    return {"redemptions": result}


@app.get("/redemptions/{redemption_id}")
async def get_redemption(redemption_id: str):
    """Get redemption details."""
    redemption = next((r for r in redemptions if r.id == redemption_id), None)
    if not redemption:
        return {"error": "Redemption not found"}

    redemption_fresh_issues = [f for f in fresh_issues if f.redemption_id == redemption_id]
    redemption_crr = next((c for c in crr_requirements if c.redemption_id == redemption_id), None)

    return {"redemption": redemption, "fresh_issues": redemption_fresh_issues, "crr_requirement": redemption_crr}


@app.get("/crr-requirements")
async def list_crr_requirements(status: Optional[str] = None):
    """List CRR requirements."""
    result = crr_requirements
    if status:
        result = [c for c in result if c.compliance_status == status]
    return {"crr_requirements": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
