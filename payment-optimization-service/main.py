"""
Payment Optimization Service
Port: 8261
Payment timing and method optimization
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

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

@app.post("/optimize", response_model=PaymentOptimizationResponse)
async def optimize_payments(request: PaymentOptimizationRequest):
    logger.info("Optimizing payments", company=request.company_id)

    payment_schedule = []
    total_discount_taken = 0
    total_discount_available = 0
    
    for p in request.payments:
        discount_rate = p.discount_available / p.amount if p.amount else 0
        annualized_cost = discount_rate / p.discount_days * 365 if p.discount_days > 0 else 0
        
        take_discount = annualized_cost < 0.20
        
        if take_discount and p.amount <= request.available_cash:
            action = "PAY_EARLY"
            payment_date = f"Early (discount)"
            savings = p.discount_available
            total_discount_taken += p.discount_available
        else:
            action = "PAY_NORMAL"
            payment_date = p.due_date
            savings = 0
        
        total_discount_available += p.discount_available
        
        payment_schedule.append({
            "payment_id": p.payment_id,
            "amount": p.amount,
            "due_date": p.due_date,
            "payment_date": payment_date,
            "action": action,
            "discount_available": p.discount_available,
            "savings": round(savings, 2),
            "annualized_cost_pct": round(annualized_cost * 100, 2)
        })
    
    savings_analysis = {
        "total_payments": len(request.payments),
        "total_amount": round(sum(p.amount for p in request.payments), 2),
        "discount_available": round(total_discount_available, 2),
        "discount_taken": round(total_discount_taken, 2),
        "discount_utilization": round(total_discount_taken / total_discount_available * 100, 2) if total_discount_available else 0
    }
    
    recommendations = []
    if savings_analysis["discount_utilization"] < 50:
        recommendations.append("Low discount utilization - consider paying invoices early to capture discounts")
    if request.available_cash < sum(p.amount for p in request.payments) * 0.3:
        recommendations.append("Limited cash - prioritize high-value discounts")

    return PaymentOptimizationResponse(
        company_id=request.company_id,
        payment_schedule=payment_schedule,
        savings_analysis=savings_analysis,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8261)
