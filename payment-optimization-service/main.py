"""
Payment Optimization Service
Port: 8261
Payment timing and method optimization
"""

import importlib.util
import os
import sys
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("payment_optimization_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["payment_optimization_calc"] = calc
_spec.loader.exec_module(calc)

logger = structlog.get_logger()
app = FastAPI(title="Payment Optimization Service", version="1.0.0")


class Payment(BaseModel):
    payment_id: str
    amount: float
    due_date: str
    discount_available: float
    discount_days: int


class PaymentOptimizationRequest(BaseModel):
    company_id: str
    payments: List[Payment]
    available_cash: float
    credit_available: float


class PaymentOptimizationResponse(BaseModel):
    company_id: str
    payment_schedule: List[Dict[str, Any]]
    savings_analysis: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "payment-optimization", "version": "1.0.0"}


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    payment_schedule = []
    total_discount_taken = 0
    total_discount_available = 0

    for p in payload["payments"]:
        discount_rate = p["discount_available"] / p["amount"] if p["amount"] else 0
        annualized_cost = discount_rate / p["discount_days"] * 365 if p["discount_days"] > 0 else 0

        take_discount = annualized_cost < 0.20

        if take_discount and p["amount"] <= payload["available_cash"]:
            action = "PAY_EARLY"
            payment_date = f"Early (discount)"
            savings = p["discount_available"]
            total_discount_taken += p["discount_available"]
        else:
            action = "PAY_NORMAL"
            payment_date = p["due_date"]
            savings = 0

        total_discount_available += p["discount_available"]

        payment_schedule.append(
            {
                "payment_id": p["payment_id"],
                "amount": p["amount"],
                "due_date": p["due_date"],
                "payment_date": payment_date,
                "action": action,
                "discount_available": p["discount_available"],
                "savings": round(savings, 2),
                "annualized_cost_pct": round(annualized_cost * 100, 2),
            }
        )

    savings_analysis = {
        "total_payments": len(payload["payments"]),
        "total_amount": round(sum(p["amount"] for p in payload["payments"]), 2),
        "discount_available": round(total_discount_available, 2),
        "discount_taken": round(total_discount_taken, 2),
        "discount_utilization": (
            round(total_discount_taken / total_discount_available * 100, 2) if total_discount_available else 0
        ),
    }

    recommendations = []
    if savings_analysis["discount_utilization"] < 50:
        recommendations.append("Low discount utilization - consider paying invoices early to capture discounts")
    if payload["available_cash"] < sum(p["amount"] for p in payload["payments"]) * 0.3:
        recommendations.append("Limited cash - prioritize high-value discounts")

    return dict(
        company_id=payload["company_id"],
        payment_schedule=payment_schedule,
        savings_analysis=savings_analysis,
        recommendations=recommendations,
    )


@app.post("/optimize", response_model=PaymentOptimizationResponse)
async def optimize_payments(request: PaymentOptimizationRequest):
    logger.info("Calculating", company=request.company_id)
    result = calc.optimize(request.model_dump(), _reference)
    return PaymentOptimizationResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8261)
