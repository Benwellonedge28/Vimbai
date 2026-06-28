"""
Project Accounting Service
Port: 8352
Project cost tracking and profitability
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Project Accounting Service", version="1.0.0")

class ProjectCostRequest(BaseModel):
    project_id: str
    costs: List[Dict[str, Any]]
    labor_hours: Dict[str, float]
    billable_amounts: Dict[str, float]

class ProjectCostResponse(BaseModel):
    project_id: str
    total_cost: float
    total_billable: float
    profit: float
    margin: float
    cost_breakdown: Dict[str, float]
    utilization_rate: float

class ProjectBudgetRequest(BaseModel):
    project_id: str
    budget_items: List[Dict[str, Any]]
    contingencies: float

class ProjectBudgetResponse(BaseModel):
    project_id: str
    total_budget: float
    allocated: float
    contingency: float
    remaining: float
    variance_alerts: List[str]

class WIPRequest(BaseModel):
    project_id: str
    recognized_revenue: float
    total_revenue: float
    costs_incurred: float

class WIPResponse(BaseModel):
    project_id: str
    wip_value: float
    billable_wip: float
    recognized_to_date: float
    pending_revenue: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "project-accounting", "version": "1.0.0"}

@app.post("/costs", response_model=ProjectCostResponse)
async def calculate_project_costs(request: ProjectCostRequest):
    logger.info("Calculating project costs", project=request.project_id)
    
    total_cost = sum(c.get("amount", 0) for c in request.costs)
    labor_cost = sum(h * 75 for h in request.labor_hours.values())
    total_billable = sum(request.billable_amounts.values())
    
    return ProjectCostResponse(
        project_id=request.project_id,
        total_cost=round(total_cost + labor_cost, 2),
        total_billable=round(total_billable, 2),
        profit=round(total_billable - total_cost - labor_cost, 2),
        margin=round((total_billable - total_cost) / total_billable * 100 if total_billable else 0, 2),
        cost_breakdown={"materials": total_cost, "labor": labor_cost},
        utilization_rate=85.5
    )

@app.post("/budget", response_model=ProjectBudgetResponse)
async def create_project_budget(request: ProjectBudgetRequest):
    logger.info("Creating project budget", project=request.project_id)
    
    allocated = sum(b.get("amount", 0) for b in request.budget_items)
    
    return ProjectBudgetResponse(
        project_id=request.project_id,
        total_budget=round(allocated + request.contingencies, 2),
        allocated=round(allocated, 2),
        contingency=round(request.contingencies, 2),
        remaining=round(request.contingencies, 2),
        variance_alerts=[]
    )

@app.post("/wip", response_model=WIPResponse)
async def calculate_wip(request: WIPRequest):
    logger.info("Calculating WIP", project=request.project_id)
    
    return WIPResponse(
        project_id=request.project_id,
        wip_value=round(request.costs_incurred, 2),
        billable_wip=round(request.costs_incurred * 1.3, 2),
        recognized_to_date=round(request.recognized_revenue, 2),
        pending_revenue=round(request.total_revenue - request.recognized_revenue, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8352)
