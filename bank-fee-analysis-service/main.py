"""
Vimbai Bank Fee Analysis Service
Bank charge tracking, fee benchmarking, and optimization recommendations.
Port: 8391
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "bank-fee-analysis-service"
PORT = int(os.getenv("PORT", "8391"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Bank Fee Analysis Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class BankCharge(BaseModel):
    date: str
    description: str
    amount: float
    category: str
    # transaction, maintenance, overdraft, wire, foreign_exchange, other


class FeeAnalysisRequest(BaseModel):
    company_id: str
    period: str
    bank_name: str
    charges: List[BankCharge]
    transaction_volume: int = 0
    avg_balance: float = 0


class FeeAnalysisResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    bank_name: str
    total_fees: float
    fee_by_category: Dict[str, float]
    fee_per_transaction: float
    fee_ratio_to_balance: float
    highest_charges: List[Dict]
    recommendations: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/analyze", response_model=FeeAnalysisResult)
async def analyze_fees(req: FeeAnalysisRequest):
    total = sum(c.amount for c in req.charges)
    by_category = {}
    for c in req.charges:
        by_category[c.category] = by_category.get(c.category, 0) + c.amount

    per_txn = total / req.transaction_volume if req.transaction_volume else 0
    ratio = total / req.avg_balance * 100 if req.avg_balance else 0

    sorted_charges = sorted(req.charges, key=lambda c: c.amount, reverse=True)[:5]
    highest = [
        {"date": c.date, "description": c.description, "amount": round(c.amount, 2), "category": c.category}
        for c in sorted_charges
    ]

    recommendations = []
    if by_category.get("overdraft", 0) > total * 0.2:
        recommendations.append("Overdraft fees are significant - consider arranging an overdraft facility")
    if by_category.get("foreign_exchange", 0) > total * 0.15:
        recommendations.append("High FX charges - negotiate better rates or use multi-currency accounts")
    if by_category.get("maintenance", 0) > total * 0.1:
        recommendations.append("Review account maintenance fees - negotiate waivers based on balance levels")
    if per_txn > 2:
        recommendations.append(f"Fee per transaction ({per_txn:.2f}) is high - consider bulk processing")
    if ratio > 0.5:
        recommendations.append(f"Annual fee-to-balance ratio ({ratio:.1f}%) is high - switch to lower-cost banking")
    if not recommendations:
        recommendations.append("Bank fees are within acceptable ranges")

    return FeeAnalysisResult(
        company_id=req.company_id,
        period=req.period,
        bank_name=req.bank_name,
        total_fees=round(total, 2),
        fee_by_category={k: round(v, 2) for k, v in by_category.items()},
        fee_per_transaction=round(per_txn, 2),
        fee_ratio_to_balance=round(ratio, 2),
        highest_charges=highest,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
