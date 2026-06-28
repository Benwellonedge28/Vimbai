"""
Accounts Payable Service
Port: 8327
Accounts payable management and processing
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime, timedelta

logger = structlog.get_logger()
app = FastAPI(title="Accounts Payable Service", version="1.0.0")

class Invoice(BaseModel):
    invoice_id: str
    vendor_id: str
    vendor_name: str
    invoice_number: str
    invoice_date: str
    due_date: str
    amount: float
    tax_amount: float
    status: str
    payment_terms: str
    line_items: List[Dict[str, Any]]

class APAnalysisRequest(BaseModel):
    company_id: str
    invoices: List[Invoice]
    payment_batch_size: int
    early_payment_discount_rate: float
    available_cash: float
    credit_limit: float

class APAnalysisResponse(BaseModel):
    company_id: str
    invoice_summary: Dict[str, Any]
    aging_analysis: List[Dict[str, Any]]
    payment_schedule: List[Dict[str, Any]]
    cash_requirements: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "accounts-payable", "version": "1.0.0"}

@app.post("/analyze", response_model=APAnalysisResponse)
async def analyze_accounts_payable(request: APAnalysisRequest):
    logger.info("Analyzing accounts payable", company=request.company_id)
    
    total_outstanding = sum(inv.amount + inv.tax_amount for inv in request.invoices)
    current_inv = [inv for inv in request.invoices if inv.status == "Current"]
    overdue_inv = [inv for inv in request.invoices if inv.status == "Overdue"]
    
    aging_0_30 = sum(inv.amount for inv in request.invoices if is_within_days(inv.due_date, 30))
    aging_31_60 = sum(inv.amount for inv in request.invoices if is_within_days(inv.due_date, 60))
    aging_61_90 = sum(inv.amount for inv in request.invoices if is_within_days(inv.due_date, 90))
    aging_over_90 = total_outstanding - aging_0_30 - aging_31_60 - aging_61_90
    
    aging_analysis = [
        {"bucket": "0-30 days", "amount": round(aging_0_30, 2), "percentage": round(aging_0_30 / total_outstanding * 100, 2) if total_outstanding else 0},
        {"bucket": "31-60 days", "amount": round(aging_31_60, 2), "percentage": round(aging_31_60 / total_outstanding * 100, 2) if total_outstanding else 0},
        {"bucket": "61-90 days", "amount": round(aging_61_90, 2), "percentage": round(aging_61_90 / total_outstanding * 100, 2) if total_outstanding else 0},
        {"bucket": "Over 90 days", "amount": round(aging_over_90, 2), "percentage": round(aging_over_90 / total_outstanding * 100, 2) if total_outstanding else 0}
    ]
    
    sorted_invoices = sorted(request.invoices, key=lambda x: x.due_date)
    payment_schedule = []
    remaining_cash = request.available_cash
    
    for inv in sorted_invoices:
        if remaining_cash >= inv.amount:
            payment_schedule.append({
                "invoice_id": inv.invoice_id,
                "vendor_name": inv.vendor_name,
                "amount": inv.amount,
                "due_date": inv.due_date,
                "status": "Scheduled for Payment",
                "priority": "Normal"
            })
            remaining_cash -= inv.amount
        else:
            payment_schedule.append({
                "invoice_id": inv.invoice_id,
                "vendor_name": inv.vendor_name,
                "amount": inv.amount,
                "due_date": inv.due_date,
                "status": "Pending Cash",
                "priority": "High"
            })
    
    cash_requirements = {
        "total_payables": round(total_outstanding, 2),
        "available_cash": request.available_cash,
        "cash_shortfall": round(max(0, total_outstanding - request.available_cash), 2),
        "credit_utilization": round((total_outstanding / request.credit_limit) * 100, 2) if request.credit_limit else 0
    }
    
    recommendations = []
    if len(overdue_inv) > 0:
        recommendations.append(f"{len(overdue_inv)} overdue invoices require immediate attention")
    if cash_requirements["cash_shortfall"] > 0:
        recommendations.append("Consider using credit facility to cover cash shortfall")
    if aging_over_90 > total_outstanding * 0.1:
        recommendations.append("High overdue amounts - review collection process")

    return APAnalysisResponse(
        company_id=request.company_id,
        invoice_summary={
            "total_invoices": len(request.invoices),
            "total_outstanding": round(total_outstanding, 2),
            "current_invoices": len(current_inv),
            "overdue_invoices": len(overdue_inv)
        },
        aging_analysis=aging_analysis,
        payment_schedule=payment_schedule,
        cash_requirements=cash_requirements,
        recommendations=recommendations
    )

def is_within_days(due_date: str, days: int) -> bool:
    try:
        due = datetime.strptime(due_date, "%Y-%m-%d")
        return (datetime.now() - due).days <= days
    except:
        return False

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8327)
