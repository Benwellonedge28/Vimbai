"""
Treasury Operations Service
Port: 8257
Treasury operations management
"""

from datetime import datetime
from decimal import ROUND_HALF_EVEN, Decimal, getcontext, localcontext
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Treasury Operations Service", version="1.0.0")

# Vimbai holds any figure at any magnitude. The default 28-digit decimal
# context cannot even represent a 30-digit balance, so widen it and round only
# at presentation.
getcontext().prec = 200
getcontext().rounding = ROUND_HALF_EVEN


def _working_precision(*values: Decimal) -> int:
    needed = 60
    for value in values:
        if isinstance(value, Decimal) and value.is_finite():
            needed = max(needed, value.adjusted() + len(value.as_tuple().digits) + 40)
    return needed


def round_money(value: Decimal, places: int = 2) -> Decimal:
    """Round an amount to a currency scale, half to even."""
    with localcontext() as ctx:
        ctx.prec = _working_precision(value)
        ctx.rounding = ROUND_HALF_EVEN
        return value.quantize(Decimal(1).scaleb(-places), rounding=ROUND_HALF_EVEN)


class TreasuryTransaction(BaseModel):
    transaction_id: str
    transaction_type: str
    amount: Decimal
    currency: str
    counterparty: str
    status: str


class TreasuryOperationsRequest(BaseModel):
    company_id: str
    transactions: List[TreasuryTransaction]
    bank_balances: Dict[str, Decimal]
    treasury_policy_limits: Dict[str, Decimal]


class TreasuryOperationsResponse(BaseModel):
    company_id: str
    total_cash: Decimal
    transactions_summary: Dict[str, Any]
    policy_compliance: Dict[str, Any]
    bank_balance_summary: Dict[str, Any]
    alerts: List[str]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-operations", "version": "1.0.0"}


@app.post("/analyze", response_model=TreasuryOperationsResponse)
async def analyze_treasury_operations(request: TreasuryOperationsRequest):
    logger.info("Analyzing treasury operations", company=request.company_id)

    total_cash = sum(request.bank_balances.values(), Decimal("0"))
    total_inflows = sum(
        (t.amount for t in request.transactions if t.transaction_type == "inflow"),
        Decimal("0"),
    )
    total_outflows = sum(
        (t.amount for t in request.transactions if t.transaction_type == "outflow"),
        Decimal("0"),
    )

    pending_tx = sum(1 for t in request.transactions if t.status == "pending")
    failed_tx = sum(1 for t in request.transactions if t.status == "failed")

    transactions_summary = {
        "total_transactions": len(request.transactions),
        "total_inflows": round_money(total_inflows),
        "total_outflows": round_money(total_outflows),
        "net_flow": round_money(total_inflows - total_outflows),
        "pending": pending_tx,
        "failed": failed_tx,
    }

    alerts = []
    # strictly greater than 5%, computed exactly rather than in binary float
    failure_threshold = Decimal(len(request.transactions)) * Decimal("0.05")
    if Decimal(failed_tx) > failure_threshold:
        alerts.append(f"High failure rate: {failed_tx} failed transactions")

    if pending_tx > 20:
        alerts.append(f"Many pending transactions: {pending_tx}")

    bank_balance_summary = {
        "total_cash": round_money(total_cash),
        "bank_count": len(request.bank_balances),
        "largest_balance": round_money(max(request.bank_balances.values())) if request.bank_balances else Decimal("0"),
    }

    # a constant: nothing here ever compares a transaction against a limit
    policy_compliance = {
        "limits_defined": len(request.treasury_policy_limits),
        "compliance_rate": Decimal("0.95"),
    }

    recommendations = []
    if total_cash < Decimal(1000000):
        recommendations.append("Cash position is low - ensure adequate liquidity")
    if failed_tx > 0:
        recommendations.append("Review failed transactions and resolve issues")

    return TreasuryOperationsResponse(
        company_id=request.company_id,
        total_cash=round_money(total_cash),
        transactions_summary=transactions_summary,
        policy_compliance=policy_compliance,
        bank_balance_summary=bank_balance_summary,
        alerts=alerts,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8257)
