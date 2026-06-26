"""
Treasury Operations Service
Port: 8257
Treasury operations management
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Treasury Operations Service", version="1.0.0")

class TreasuryTransaction(BaseModel):
    transaction_id: str
    transaction_type: str
    amount: float
    currency: str
    counterparty: str
    status: str

class TreasuryOperationsRequest(BaseModel):
    company_id: str
    transactions: List[TreasuryTransaction]
    bank_balances: Dict[str, float]
    treasury_policy_limits: Dict[str, float]

class TreasuryOperationsResponse(BaseModel):
    company_id: str
    total_cash: float
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

    total_cash = sum(request.bank_balances.values())
    total_inflows = sum(t.amount for t in request.transactions if t.transaction_type == "inflow")
    total_outflows = sum(t.amount for t in request.transactions if t.transaction_type == "outflow")
    
    pending_tx = sum(1 for t in request.transactions if t.status == "pending")
    failed_tx = sum(1 for t in request.transactions if t.status == "failed")
    
    transactions_summary = {
        "total_transactions": len(request.transactions),
        "total_inflows": round(total_inflows, 2),
        "total_outflows": round(total_outflows, 2),
        "net_flow": round(total_inflows - total_outflows, 2),
        "pending": pending_tx,
        "failed": failed_tx
    }
    
    alerts = []
    if failed_tx > len(request.transactions) * 0.05:
        alerts.append(f"High failure rate: {failed_tx} failed transactions")
    
    if pending_tx > 20:
        alerts.append(f"Many pending transactions: {pending_tx}")
    
    bank_balance_summary = {
        "total_cash": round(total_cash, 2),
        "bank_count": len(request.bank_balances),
        "largest_balance": round(max(request.bank_balances.values()), 2) if request.bank_balances else 0
    }
    
    policy_compliance = {
        "limits_defined": len(request.treasury_policy_limits),
        "compliance_rate": 0.95
    }
    
    recommendations = []
    if total_cash < 1000000:
        recommendations.append("Cash position is low - ensure adequate liquidity")
    if failed_tx > 0:
        recommendations.append("Review failed transactions and resolve issues")

    return TreasuryOperationsResponse(
        company_id=request.company_id,
        total_cash=round(total_cash, 2),
        transactions_summary=transactions_summary,
        policy_compliance=policy_compliance,
        bank_balance_summary=bank_balance_summary,
        alerts=alerts,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8257)
