"""
Vimbai Bonus Shares Service
Manages capitalization of reserves into bonus shares.
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

SERVICE_NAME = "bonus-shares-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8050"))
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

app = FastAPI(title="Vimbai Bonus Shares Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class BonusIssue(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    issue_date: datetime
    shares_issued: int
    nominal_value: float
    total_nominal_value: float = 0
    source_reserve: str  # share_premium, retained_earnings, general_reserve
    amount_utilized: float = 0
    shareholder_allocations: Dict[str, int] = {}  # shareholder_id -> shares
    journal_entry_id: Optional[str] = None
    status: str = "approved"
    created_at: datetime = Field(default_factory=datetime.utcnow)


bonus_issues: List[BonusIssue] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Bonus shares issuance"}


@app.post("/issue")
async def issue_bonus_shares(
    company_id: str,
    issue_date: datetime,
    shares_issued: int,
    nominal_value: float,
    source_reserve: str,
    shareholder_allocations: Dict[str, int],
):
    """Issue bonus shares from reserves."""
    issue = BonusIssue(
        company_id=company_id,
        issue_date=issue_date,
        shares_issued=shares_issued,
        nominal_value=nominal_value,
        source_reserve=source_reserve,
        shareholder_allocations=shareholder_allocations,
    )
    issue.total_nominal_value = shares_issued * nominal_value
    issue.amount_utilized = issue.total_nominal_value

    reserve_account = {"share_premium": "3210", "retained_earnings": "3300", "general_reserve": "3310"}.get(
        source_reserve, "3300"
    )

    journal_entry = {
        "date": issue_date,
        "description": f"Issue of {shares_issued} bonus shares from {source_reserve}",
        "entries": [
            {
                "account_code": reserve_account,
                "description": f"{source_reserve} Reserve",
                "debit": issue.amount_utilized,
                "credit": 0,
            },
            {"account_code": "3200", "description": "Share Capital", "debit": 0, "credit": issue.total_nominal_value},
        ],
        "reference": f"BONUS-{issue.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    issue.journal_entry_id = result.get("id")
    bonus_issues.append(issue)
    return issue


@app.get("/issues")
async def list_bonus_issues(company_id: Optional[str] = None):
    """List bonus issues."""
    result = bonus_issues
    if company_id:
        result = [i for i in result if i.company_id == company_id]
    return {"issues": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
