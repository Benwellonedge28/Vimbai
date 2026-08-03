"""
Vimbai Issued Share Capital Service
Manages issued shares and allotments.
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

SERVICE_NAME = "issued-share-capital-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8048"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Issued Share Capital Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class Shareholder(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    address: str
    shareholder_type: str = "individual"
    shares_held: int = 0
    percentage: float = 0


class ShareIssue(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    issue_date: datetime
    share_class: str
    shares_issued: int
    issue_price: float
    total_proceeds: float = 0
    shareholders: List[Dict[str, Any]] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


shareholders: Dict[str, List[Shareholder]] = {}
issues: List[ShareIssue] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Issued share capital management"}


@app.post("/issue")
async def issue_shares(
    company_id: str, issue_date: datetime, share_class: str, shares_issued: int, issue_price: float
):
    """Issue new shares."""
    issue = ShareIssue(
        company_id=company_id, issue_date=issue_date, share_class=share_class,
        shares_issued=shares_issued, issue_price=issue_price
    )
    issue.total_proceeds = shares_issued * issue_price

    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {shares_issued} {share_class} shares at {issue_price}",
        "entries": [
            {"account_code": "1000", "description": "Bank", "debit": issue.total_proceeds, "credit": 0},
            {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": shares_issued * 1},
            {"account_code": "3210", "description": "Share Premium", "debit": 0, "credit": issue.total_proceeds - shares_issued},
        ],
        "reference": f"ISS-{issue.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    issue.journal_entry_id = result.get("id")
    issues.append(issue)
    return issue


@app.post("/shareholders/register")
async def register_shareholder(company_id: str, name: str, address: str, shares_held: int = 0):
    """Register a shareholder."""
    if company_id not in shareholders:
        shareholders[company_id] = []

    holder = Shareholder(name=name, address=address, shares_held=shares_held)
    shareholders[company_id].append(holder)
    return holder


@app.get("/shareholders/{company_id}")
async def get_shareholders(company_id: str):
    """Get company shareholders."""
    return {"shareholders": shareholders.get(company_id, [])}


@app.get("/issues/{company_id}")
async def get_share_issues(company_id: str):
    """Get all share issues for a company."""
    return {"issues": [i for i in issues if i.company_id == company_id]}


@app.get("/summary/{company_id}")
async def get_capital_summary(company_id: str):
    """Get share capital summary."""
    company_issues = [i for i in issues if i.company_id == company_id]
    total_shares = sum(i.shares_issued for i in company_issues)
    total_capital = sum(i.total_proceeds for i in company_issues)
    return {"total_shares_issued": total_shares, "total_proceeds": total_capital}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)