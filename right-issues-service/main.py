"""
Vimbai Right Issues Service
Manages rights issues of shares to existing shareholders.
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

SERVICE_NAME = "right-issues-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8051"))
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

app = FastAPI(title="Vimbai Right Issues Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class RightIssue(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    issue_date: datetime
    expiry_date: datetime
    share_class: str
    rights_offered: int  # rights per existing share
    new_shares_offered: int
    issue_price: float
    nominal_value: float
    total_proceeds: float = 0
    renouncement_allowed: bool = True
    entitlements: Dict[str, int] = {}  # shareholder_id -> rights entitlement
    acceptances: Dict[str, int] = {}  # shareholder_id -> shares accepted
    renunciations: Dict[str, str] = {}  # shareholder_id -> new_owner_id
    journal_entry_id: Optional[str] = None
    status: str = "offered"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class Renunciation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    right_issue_id: str
    original_shareholder_id: str
    new_shareholder_id: str
    renunciation_date: datetime
    renunciation_price: float


right_issues: List[RightIssue] = []
renunciations: List[Renunciation] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Right issues management"}


@app.post("/offer")
async def offer_rights(
    company_id: str,
    issue_date: datetime,
    expiry_date: datetime,
    share_class: str,
    rights_offered: int,
    new_shares_offered: int,
    issue_price: float,
    nominal_value: float,
    entitlements: Dict[str, int],
    renouncement_allowed: bool = True,
):
    """Offer rights issue to existing shareholders."""
    right_issue = RightIssue(
        company_id=company_id,
        issue_date=issue_date,
        expiry_date=expiry_date,
        share_class=share_class,
        rights_offered=rights_offered,
        new_shares_offered=new_shares_offered,
        issue_price=issue_price,
        nominal_value=nominal_value,
        entitlements=entitlements,
        renouncement_allowed=renouncement_allowed,
    )
    right_issue.total_proceeds = new_shares_offered * issue_price
    right_issues.append(right_issue)
    return right_issue


@app.post("/{right_issue_id}/accept")
async def accept_rights(right_issue_id: str, shareholder_id: str, shares_accepted: int):
    """Record acceptance of rights issue."""
    right_issue = next((r for r in right_issues if r.id == right_issue_id), None)
    if not right_issue:
        return {"error": "Right issue not found"}

    right_issue.acceptances[shareholder_id] = shares_accepted

    acceptance_value = shares_accepted * right_issue.issue_price
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Acceptance of rights issue - {shares_accepted} shares",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": acceptance_value, "credit": 0},
            {
                "account_code": "3200",
                "description": "Share Capital",
                "debit": 0,
                "credit": shares_accepted * right_issue.nominal_value,
            },
            {
                "account_code": "3210",
                "description": "Share Premium",
                "debit": 0,
                "credit": acceptance_value - (shares_accepted * right_issue.nominal_value),
            },
        ],
        "reference": f"RIGHT-ACCEPT-{right_issue_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    if all(
        right_issue.entitlements.get(sh, 0)
        == right_issue.acceptances.get(sh, 0) + sum(1 for orig, new in right_issue.renunciations.items() if orig == sh)
        for sh in right_issue.entitlements
    ):
        right_issue.status = "completed"

    return right_issue


@app.post("/{right_issue_id}/renounce")
async def renounce_rights(
    right_issue_id: str,
    shareholder_id: str,
    new_shareholder_id: str,
    renunciation_price: float,
    renunciation_date: datetime,
):
    """Record renunciation of rights to another party."""
    right_issue = next((r for r in right_issues if r.id == right_issue_id), None)
    if not right_issue:
        return {"error": "Right issue not found"}

    if not right_issue.renouncement_allowed:
        return {"error": "Renouncement not allowed for this issue"}

    renunciation = Renunciation(
        right_issue_id=right_issue_id,
        original_shareholder_id=shareholder_id,
        new_shareholder_id=new_shareholder_id,
        renunciation_date=renunciation_date,
        renunciation_price=renunciation_price,
    )
    right_issue.renunciations[shareholder_id] = new_shareholder_id
    renunciations.append(renunciation)

    # Journal entry for renunciation consideration
    journal_entry = {
        "date": renunciation_date,
        "description": f"Rights renunciation payment",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": renunciation_price, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": renunciation_price},
        ],
        "reference": f"RIGHT-REN-{right_issue_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    return {"right_issue": right_issue, "renunciation": renunciation}


@app.get("/right-issues")
async def list_right_issues(company_id: Optional[str] = None):
    """List all right issues."""
    result = right_issues
    if company_id:
        result = [r for r in result if r.company_id == company_id]
    return {"right_issues": result}


@app.get("/{right_issue_id}")
async def get_right_issue(right_issue_id: str):
    """Get right issue details."""
    right_issue = next((r for r in right_issues if r.id == right_issue_id), None)
    if not right_issue:
        return {"error": "Right issue not found"}
    return right_issue


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
