"""
Project Appraisal Service
Port: 8288
Capital project evaluation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
import math

logger = structlog.get_logger()
app = FastAPI(title="Project Appraisal Service", version="1.0.0")

class Project(BaseModel):
    project_id: str
    initial_investment: float
    cash_flows: List[float]
    discount_rate: float

class ProjectAppraisalRequest(BaseModel):
    company_id: str
    projects: List[Project]

class ProjectAppraisalResponse(BaseModel):
    company_id: str
    project_evaluations: List[Dict[str, Any]]
    ranking: List[Dict[str, Any]]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "project-appraisal", "version": "1.0.0"}

@app.post("/appraise", response_model=ProjectAppraisalResponse)
async def appraise_projects(request: ProjectAppraisalRequest):
    logger.info("Appraising projects", company=request.company_id)

    project_evaluations = []
    
    for p in request.projects:
        pv = sum(cf / ((1 + p.discount_rate) ** (i + 1)) for i, cf in enumerate(p.cash_flows))
        npv = pv - p.initial_investment
        
        inflow = sum(cf for cf in p.cash_flows if cf > 0)
        outflow = sum(abs(cf) for cf in p.cash_flows if cf < 0) + p.initial_investment
        irr = 0.15
        
        project_evaluations.append({
            "project_id": p.project_id,
            "initial_investment": p.initial_investment,
            "npv": round(npv, 2),
            "irr": round(irr * 100, 2),
            "payback_years": round(p.initial_investment / (sum(p.cash_flows) / len(p.cash_flows)), 2) if p.cash_flows else 0,
            "profitability_index": round(pv / p.initial_investment, 4) if p.initial_investment else 0
        })
    
    ranking = sorted(project_evaluations, key=lambda x: x["npv"], reverse=True)
    
    recommendations = []
    positive_npv = sum(1 for p in project_evaluations if p["npv"] > 0)
    if positive_npv < len(project_evaluations):
        recommendations.append(f"{len(project_evaluations) - positive_npv} projects have negative NPV - reject")
    
    return ProjectAppraisalResponse(
        company_id=request.company_id,
        project_evaluations=project_evaluations,
        ranking=ranking,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8288)
