"""
Tax Planning Service
Port: 8292
Strategic tax planning
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Tax Planning Service", version="1.0.0")

class TaxPlanningRequest(BaseModel):
    company_id: str
    taxable_income: float
    tax_rate: float
    deductions_available: float
    jurisdictions: List[str]

class TaxPlanningResponse(BaseModel):
    company_id: str
    tax_summary: Dict[str, Any]
    planning_options: List[Dict[str, Any]]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "tax-planning", "version": "1.0.0"}

@app.post("/plan", response_model=TaxPlanningResponse)
async def plan_taxes(request: TaxPlanningRequest):
    logger.info("Planning taxes", company=request.company_id)

    gross_tax = request.taxable_income * request.tax_rate
    after_deductions = (request.taxable_income - request.deductions_available) * request.tax_rate
    tax_savings = gross_tax - after_deductions
    
    planning_options = [
        {"option": "Accelerate Deductions", "savings": round(tax_savings * 0.3, 2)},
        {"option": "Defer Income", "savings": round(tax_savings * 0.2, 2)},
        {"option": "Entity Restructuring", "savings": round(tax_savings * 0.25, 2)}
    ]
    
    tax_summary = {
        "taxable_income": request.taxable_income,
        "gross_tax": round(gross_tax, 2),
        "deductions": request.deductions_available,
        "net_tax": round(after_deductions, 2),
        "effective_rate": round(request.tax_rate * 100, 2)
    }
    
    recommendations = []
    if tax_savings > 100000:
        recommendations.append("Significant tax planning opportunity - implement strategies")
    
    return TaxPlanningResponse(
        company_id=request.company_id,
        tax_summary=tax_summary,
        planning_options=planning_options,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8292)
