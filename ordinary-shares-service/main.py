"""
Vimbai Ordinary Shares Service
Ordinary share management and dividend distribution.
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

SERVICE_NAME = "ordinary-shares-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8049"))
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

app = FastAPI(title="Vimbai Ordinary Shares Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class OrdinaryDividend(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    dividend_type: str  # interim, final, special
    per_share_amount: float
    total_shares: int
    total_dividend: float = 0
    record_date: datetime
    payment_date: Optional[datetime] = None
    status: str = "declared"
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


dividends: List[OrdinaryDividend] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Ordinary shares management"}


@app.post("/dividends/declare")
async def declare_dividend(
    company_id: str, dividend_type: str, per_share_amount: float, total_shares: int, record_date: datetime
):
    """Declare ordinary dividend."""
    dividend = OrdinaryDividend(
        company_id=company_id,
        dividend_type=dividend_type,
        per_share_amount=per_share_amount,
        total_shares=total_shares,
        record_date=record_date,
    )
    dividend.total_dividend = per_share_amount * total_shares

    journal_entry = {
        "date": record_date,
        "description": f"Declaration of {dividend_type} dividend",
        "entries": [
            {"account_code": "3300", "description": "Retained Earnings", "debit": dividend.total_dividend, "credit": 0},
            {"account_code": "2310", "description": "Dividend Payable", "debit": 0, "credit": dividend.total_dividend},
        ],
        "reference": f"DIV-{dividend.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.journal_entry_id = result.get("id")
    dividends.append(dividend)
    return dividend


@app.post("/dividends/{dividend_id}/pay")
async def pay_dividend(dividend_id: str, payment_date: datetime):
    """Pay dividend to shareholders."""
    dividend = next((d for d in dividends if d.id == dividend_id), None)
    if not dividend:
        return {"error": "Dividend not found"}

    journal_entry = {
        "date": payment_date,
        "description": f"Payment of {dividend.dividend_type} dividend",
        "entries": [
            {"account_code": "2310", "description": "Dividend Payable", "debit": dividend.total_dividend, "credit": 0},
            {"account_code": "1000", "description": "Bank", "debit": 0, "credit": dividend.total_dividend},
        ],
        "reference": f"DIV-PAY-{dividend_id[:8]}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)
    dividend.payment_date = payment_date
    dividend.status = "paid"
    return dividend


@app.get("/dividends")
async def list_dividends(company_id: Optional[str] = None):
    """List dividends."""
    result = dividends
    if company_id:
        result = [d for d in result if d.company_id == company_id]
    return {"dividends": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
