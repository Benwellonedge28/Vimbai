"""
Budget Variance Service
Port: 8277
Budget variance analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Budget Variance Service", version="1.0.0")

class BudgetItem(BaseModel):
    item_id: str
    item_name: str
    category: str
    budget: float
    actual: float

class BudgetVarianceRequest(BaseModel):
    company_id: str
    period: str
    items: List[BudgetItem]
    cost_center: str

class BudgetVarianceResponse(BaseModel):
    company_id: str
    period: str
    variance_summary: Dict[str, Any]
    item_analysis: List[Dict[str, Any]]
    significant_variances: List[Dict[str, Any]]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "budget-variance", "version": "1.0.0"}

@app.post("/analyze", response_model=BudgetVarianceResponse)
async def analyze_budget_variance(request: BudgetVarianceRequest):
    logger.info("Analyzing budget variance", company=request.company_id)

    item_analysis = []
    significant_variances = []
    total_budget = 0
    total_actual = 0
    
    for item in request.items:
        variance = item.actual - item.budget
        variance_pct = (variance / item.budget * 100) if item.budget else 0
        
        item_analysis.append({
            "item_id": item.item_id,
            "item_name": item.item_name,
            "category": item.category,
            "budget": round(item.budget, 2),
            "actual": round(item.actual, 2),
            "variance": round(variance, 2),
            "variance_pct": round(variance_pct, 2),
            "favorable": variance < 0
        })
        
        total_budget += item.budget
        total_actual += item.actual
        
        if abs(variance_pct) > 10:
            significant_variances.append({
                "item": item.item_name,
                "variance": round(variance, 2),
                "variance_pct": round(variance_pct, 2)
            })
    
    total_variance = total_actual - total_budget
    
    variance_summary = {
        "total_budget": round(total_budget, 2),
        "total_actual": round(total_actual, 2),
        "total_variance": round(total_variance, 2),
        "variance_pct": round((total_variance / total_budget * 100), 2) if total_budget else 0,
        "cost_center": request.cost_center
    }
    
    recommendations = []
    if abs(variance_summary["variance_pct"]) > 5:
        recommendations.append("Overall budget variance exceeds 5% - investigate drivers")
    if len(significant_variances) > 5:
        recommendations.append("Multiple significant variances - review exception items")

    return BudgetVarianceResponse(
        company_id=request.company_id,
        period=request.period,
        variance_summary=variance_summary,
        item_analysis=item_analysis,
        significant_variances=significant_variances,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8277)
