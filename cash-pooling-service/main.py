"""
Cash Pooling Service
Port: 8258
Cash concentration and pooling analysis
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Cash Pooling Service", version="1.0.0")


class Account(BaseModel):
    account_id: str
    account_name: str
    balance: float
    currency: str
    bank: str
    is_pooled: bool


class CashPoolingRequest(BaseModel):
    company_id: str
    accounts: List[Account]
    pooling_type: str
    target_account: str
    threshold_amount: float


class CashPoolingResponse(BaseModel):
    company_id: str
    pool_summary: Dict[str, Any]
    account_analysis: List[Dict[str, Any]]
    optimization_opportunities: List[Dict[str, Any]]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "cash-pooling", "version": "1.0.0"}


@app.post("/analyze", response_model=CashPoolingResponse)
async def analyze_cash_pooling(request: CashPoolingRequest):
    logger.info("Analyzing cash pooling", company=request.company_id)

    pooled_accounts = [a for a in request.accounts if a.is_pooled]
    non_pooled_accounts = [a for a in request.accounts if not a.is_pooled]

    total_pooled = sum(a.balance for a in pooled_accounts)
    total_non_pooled = sum(a.balance for a in non_pooled_accounts)
    total_cash = sum(a.balance for a in request.accounts)

    excess_cash = max(0, total_non_pooled - request.threshold_amount)

    pool_summary = {
        "total_accounts": len(request.accounts),
        "pooled_accounts": len(pooled_accounts),
        "non_pooled_accounts": len(non_pooled_accounts),
        "total_pooled_cash": round(total_pooled, 2),
        "total_non_pooled_cash": round(total_non_pooled, 2),
        "total_cash": round(total_cash, 2),
        "pooling_efficiency": round(total_pooled / total_cash * 100, 2) if total_cash else 0,
    }

    account_analysis = []
    for acc in request.accounts:
        account_analysis.append(
            {
                "account_id": acc.account_id,
                "account_name": acc.account_name,
                "balance": round(acc.balance, 2),
                "currency": acc.currency,
                "bank": acc.bank,
                "is_pooled": acc.is_pooled,
                "surplus_deficit": round(acc.balance - request.threshold_amount, 2),
            }
        )

    optimization_opportunities = []
    if excess_cash > 0:
        optimization_opportunities.append(
            {
                "type": "sweep_excess",
                "amount": round(excess_cash, 2),
                "description": f"Sweep {excess_cash:.2f} to central pool",
            }
        )

    if len(non_pooled_accounts) > 3:
        optimization_opportunities.append(
            {
                "type": "consolidate_accounts",
                "accounts_to_close": len(non_pooled_accounts) - 3,
                "description": "Consider closing redundant accounts",
            }
        )

    recommendations = []
    if pool_summary["pooling_efficiency"] < 80:
        recommendations.append("Improve pooling efficiency by including more accounts")
    if len(non_pooled_accounts) > 5:
        recommendations.append("Too many non-pooled accounts - consolidate where possible")

    return CashPoolingResponse(
        company_id=request.company_id,
        pool_summary=pool_summary,
        account_analysis=account_analysis,
        optimization_opportunities=optimization_opportunities,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8258)
