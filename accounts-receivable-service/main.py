"""
Accounts Receivable Service
Port: 8328
Accounts receivable management and collection
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Accounts Receivable Service", version="1.0.0")

class ARInvoice(BaseModel):
    invoice_id: str
    customer_id: str
    customer_name: str
    invoice_date: str
    due_date: str
    amount: float
    amount_paid: float
    status: str
    credit_limit: float

class ARAnalysisRequest(BaseModel):
    company_id: str
    invoices: List[ARInvoice]
    target_dso: int
    collection_budget: float

class ARAnalysisResponse(BaseModel):
    company_id: str
    ar_summary: Dict[str, Any]
    aging_schedule: List[Dict[str, Any]]
    dso_analysis: Dict[str, Any]
    collection_plan: List[Dict[str, Any]]
    bad_debt_provision: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "accounts-receivable", "version": "1.0.0"}

@app.post("/analyze", response_model=ARAnalysisResponse)
async def analyze_accounts_receivable(request: ARAnalysisRequest):
    logger.info("Analyzing accounts receivable", company=request.company_id)
    
    total_ar = sum(inv.amount - inv.amount_paid for inv in request.invoices)
    current_ar = sum(inv.amount - inv.amount_paid for inv in request.invoices if inv.status == "Current")
    overdue_30 = sum(inv.amount - inv.amount_paid for inv in request.invoices if inv.status == "Overdue 30")
    overdue_60 = sum(inv.amount - inv.amount_paid for inv in request.invoices if inv.status == "Overdue 60")
    overdue_90 = sum(inv.amount - inv.amount_paid for inv in request.invoices if inv.status == "Overdue 90")
    
    aging_schedule = [
        {"bucket": "Current", "amount": round(current_ar, 2), "percentage": round(current_ar / total_ar * 100, 2) if total_ar else 0},
        {"bucket": "1-30 Days Overdue", "amount": round(overdue_30, 2), "percentage": round(overdue_30 / total_ar * 100, 2) if total_ar else 0},
        {"bucket": "31-60 Days Overdue", "amount": round(overdue_60, 2), "percentage": round(overdue_60 / total_ar * 100, 2) if total_ar else 0},
        {"bucket": "61-90 Days Overdue", "amount": round(overdue_90, 2), "percentage": round(overdue_90 / total_ar * 100, 2) if total_ar else 0}
    ]
    
    avg_collection_period = 45
    dso_analysis = {
        "actual_dso": avg_collection_period,
        "target_dso": request.target_dso,
        "dso_variance": avg_collection_period - request.target_dso,
        "collection_efficiency": round((1 - avg_collection_period / request.target_dso) * 100, 2) if request.target_dso else 0
    }
    
    collection_plan = []
    high_priority = [inv for inv in request.invoices if inv.status in ["Overdue 60", "Overdue 90"]]
    for inv in high_priority[:10]:
        outstanding = inv.amount - inv.amount_paid
        collection_plan.append({
            "customer_id": inv.customer_id,
            "customer_name": inv.customer_name,
            "outstanding_amount": round(outstanding, 2),
            "days_overdue": 30,
            "priority": "High",
            "recommended_action": "Escalate to legal"
        })
    
    bad_debt_provision = {
        "current_arrears": overdue_30 * 0.05 + overdue_60 * 0.25 + overdue_90 * 0.75,
        "provision_percentage": round((overdue_30 * 5 + overdue_60 * 25 + overdue_90 * 75) / total_ar * 100, 2) if total_ar else 0
    }
    
    recommendations = []
    if avg_collection_period > request.target_dso * 1.2:
        recommendations.append("DSO significantly above target - improve collection efforts")
    if overdue_90 > total_ar * 0.15:
        recommendations.append("High 90+ day receivables - escalate collection")
    if bad_debt_provision["current_arrears"] > total_ar * 0.1:
        recommendations.append("High bad debt risk - increase provisions")

    return ARAnalysisResponse(
        company_id=request.company_id,
        ar_summary={"total_ar": round(total_ar, 2), "invoice_count": len(request.invoices)},
        aging_schedule=aging_schedule,
        dso_analysis=dso_analysis,
        collection_plan=collection_plan,
        bad_debt_provision=bad_debt_provision,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8328)
