"""
Vimbai Trial Balance Service
Generates trial balance from ledger balances.
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

SERVICE_NAME = "trial-balance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8132"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Trial Balance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class LedgerAccount(BaseModel):
    account_name: str
    account_type: str  # asset, liability, equity, revenue, expense
    debit_balance: float = 0
    credit_balance: float = 0


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal Vimbai service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Trial balance generation"}


@app.post("/generate")
async def generate_trial_balance(accounts: List[dict]):
    """
    Generate trial balance from ledger accounts.
    Debits must equal Credits for trial balance to agree.
    """
    trial_balance = []
    total_debits = 0
    total_credits = 0
    errors = []

    for acc in accounts:
        name = acc.get("account_name", "Unknown")
        acc_type = acc.get("account_type", "")
        dr = acc.get("debit_balance", 0)
        cr = acc.get("credit_balance", 0)

        # Determine which balance to show
        if acc_type in ["asset", "expense"]:
            balance = dr - cr
        else:
            balance = cr - dr

        if balance >= 0:
            trial_balance.append({
                "account_name": name,
                "account_type": acc_type,
                "debit": balance if balance > 0 else 0,
                "credit": 0
            })
            total_debits += balance if balance > 0 else 0
        else:
            trial_balance.append({
                "account_name": name,
                "account_type": acc_type,
                "debit": 0,
                "credit": abs(balance)
            })
            total_credits += abs(balance)

    is_balanced = abs(total_debits - total_credits) < 0.01

    return {
        "trial_balance": trial_balance,
        "total_debits": round(total_debits, 2),
        "total_credits": round(total_credits, 2),
        "is_balanced": is_balanced,
        "difference": round(total_debits - total_credits, 2),
        "status": "Agree" if is_balanced else "Disagree - Check balances"
    }


@app.post("/from-ledgers")
async def trial_balance_from_ledger_balances(ledger_balances: List[dict]):
    """Generate trial balance from existing ledger balances."""
    return await generate_trial_balance(ledger_balances)


@app.post("/validate")
async def validate_trial_balance(total_debits: float, total_credits: float):
    """Validate if trial balance balances."""
    diff = abs(total_debits - total_credits)
    is_valid = diff < 0.01

    return {
        "total_debits": total_debits,
        "total_credits": total_credits,
        "difference": round(diff, 2),
        "is_valid": is_valid,
        "message": "Trial balance agrees" if is_valid else f"Trial balance disagrees by {diff}"
    }


@app.post("/classification-summary")
async def trial_balance_classification_summary(accounts: List[dict]):
    """Show trial balance grouped by account classification."""
    assets = []
    liabilities = []
    equity = []
    revenues = []
    expenses = []

    for acc in accounts:
        acc_type = acc.get("account_type", "")
        balance = acc.get("debit_balance", 0) + acc.get("credit_balance", 0)

        if acc_type == "asset":
            assets.append(acc)
        elif acc_type == "liability":
            liabilities.append(acc)
        elif acc_type == "equity":
            equity.append(acc)
        elif acc_type == "revenue":
            revenues.append(acc)
        elif acc_type == "expense":
            expenses.append(acc)

    return {
        "assets": {"accounts": assets, "total": sum(a.get("debit_balance", 0) + a.get("credit_balance", 0) for a in assets)},
        "liabilities": {"accounts": liabilities, "total": sum(l.get("debit_balance", 0) + l.get("credit_balance", 0) for l in liabilities)},
        "equity": {"accounts": equity, "total": sum(e.get("debit_balance", 0) + e.get("credit_balance", 0) for e in equity)},
        "revenues": {"accounts": revenues, "total": sum(r.get("debit_balance", 0) + r.get("credit_balance", 0) for r in revenues)},
        "expenses": {"accounts": expenses, "total": sum(ex.get("debit_balance", 0) + ex.get("credit_balance", 0) for ex in expenses)}
    }


@app.post("/adjusted")
async def adjusted_trial_balance(pre_adjustment: List[dict], adjustments: List[dict]):
    """Generate adjusted trial balance with adjustments applied."""
    adjusted = pre_adjustment.copy()

    # Apply adjustments
    for adj in adjustments:
        account_name = adj.get("account_name")
        debit = adj.get("debit", 0)
        credit = adj.get("credit", 0)

        found = False
        for acc in adjusted:
            if acc.get("account_name") == account_name:
                acc["debit_balance"] = acc.get("debit_balance", 0) + debit
                acc["credit_balance"] = acc.get("credit_balance", 0) + credit
                found = True
                break

        if not found:
            adjusted.append({
                "account_name": account_name,
                "debit_balance": debit,
                "credit_balance": credit
            })

    return await generate_trial_balance(adjusted)


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
