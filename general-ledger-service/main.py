"""
General Ledger Service
Port: 8330
General ledger management and trial balance
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="General Ledger Service", version="1.0.0")


class GLAccount(BaseModel):
    account_id: str
    account_name: str
    account_type: str
    balance: float
    debit_credit: str
    parent_account: str


class GeneralLedgerRequest(BaseModel):
    company_id: str
    accounts: List[GLAccount]
    reporting_date: str


class GeneralLedgerResponse(BaseModel):
    company_id: str
    trial_balance: Dict[str, Any]
    account_analysis: List[Dict[str, Any]]
    balance_sheet_totals: Dict[str, float]
    income_statement_totals: Dict[str, float]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "general-ledger", "version": "1.0.0"}


@app.post("/analyze", response_model=GeneralLedgerResponse)
async def analyze_general_ledger(request: GeneralLedgerRequest):
    logger.info("Analyzing general ledger", company=request.company_id)

    total_debits = sum(a.balance for a in request.accounts if a.debit_credit == "Debit")
    total_credits = sum(a.balance for a in request.accounts if a.debit_credit == "Credit")

    asset_accounts = [a for a in request.accounts if a.account_type == "Asset"]
    liability_accounts = [a for a in request.accounts if a.account_type == "Liability"]
    equity_accounts = [a for a in request.accounts if a.account_type == "Equity"]
    revenue_accounts = [a for a in request.accounts if a.account_type == "Revenue"]
    expense_accounts = [a for a in request.accounts if a.account_type == "Expense"]

    balance_sheet_totals = {
        "total_assets": sum(a.balance for a in asset_accounts),
        "total_liabilities": sum(a.balance for a in liability_accounts),
        "total_equity": sum(a.balance for a in equity_accounts),
    }

    income_statement_totals = {
        "total_revenue": sum(a.balance for a in revenue_accounts),
        "total_expenses": sum(a.balance for a in expense_accounts),
    }

    trial_balance = {
        "total_debits": round(total_debits, 2),
        "total_credits": round(total_credits, 2),
        "balanced": abs(total_debits - total_credits) < 0.01,
        "variance": round(abs(total_debits - total_credits), 2),
    }

    account_analysis = [
        {"account_id": a.account_id, "name": a.account_name, "type": a.account_type, "balance": round(a.balance, 2)}
        for a in request.accounts
    ]

    recommendations = []
    if not trial_balance["balanced"]:
        recommendations.append("Trial balance is not balanced - review entries")
    if income_statement_totals["total_revenue"] < income_statement_totals["total_expenses"]:
        recommendations.append("Net loss detected - review expense controls")

    return GeneralLedgerResponse(
        company_id=request.company_id,
        trial_balance=trial_balance,
        account_analysis=account_analysis,
        balance_sheet_totals={k: round(v, 2) for k, v in balance_sheet_totals.items()},
        income_statement_totals={k: round(v, 2) for k, v in income_statement_totals.items()},
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8330)
