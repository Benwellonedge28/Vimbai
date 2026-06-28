"""
Benefits Administration Service
Port: 8360
Employee benefits management
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Benefits Administration Service", version="1.0.0")

class BenefitsEnrollmentRequest(BaseModel):
    company_id: str
    employee_id: str
    plan_type: str
    coverage_level: str
    dependents: List[Dict[str, Any]]

class BenefitsEnrollmentResponse(BaseModel):
    enrollment_id: str
    employee_id: str
    plan_name: str
    employee_cost: float
    employer_cost: float
    effective_date: date

class BenefitsCostRequest(BaseModel):
    company_id: str
    plan_type: str
    employee_count: int
    coverage_options: Dict[str, float]

class BenefitsCostResponse(BaseModel):
    company_id: str
    plan_type: str
    total_employee_cost: float
    total_employer_cost: float
    cost_per_employee: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "benefits-admin", "version": "1.0.0"}

@app.post("/enroll", response_model=BenefitsEnrollmentResponse)
async def enroll_in_benefits(request: BenefitsEnrollmentRequest):
    logger.info("Enrolling in benefits", company=request.company_id, employee=request.employee_id)
    
    costs = {"individual": 50, "family": 200, "employee_spouse": 150}
    emp_cost = costs.get(request.coverage_level, 100) * 1.5
    er_cost = costs.get(request.coverage_level, 100) * 2.5
    
    return BenefitsEnrollmentResponse(
        enrollment_id=f"ENR-{datetime.now().strftime('%Y%m%d%H%M')}",
        employee_id=request.employee_id,
        plan_name=f"{request.plan_type.title()} Plan",
        employee_cost=round(emp_cost, 2),
        employer_cost=round(er_cost, 2),
        effective_date=date.today()
    )

@app.post("/costs", response_model=BenefitsCostResponse)
async def calculate_benefits_costs(request: BenefitsCostRequest):
    logger.info("Calculating benefits costs", company=request.company_id, plan=request.plan_type)
    
    total_emp_cost = sum(request.coverage_options.values()) * request.employee_count * 0.4
    total_er_cost = sum(request.coverage_options.values()) * request.employee_count * 0.6
    
    return BenefitsCostResponse(
        company_id=request.company_id,
        plan_type=request.plan_type,
        total_employee_cost=round(total_emp_cost, 2),
        total_employer_cost=round(total_er_cost, 2),
        cost_per_employee=round((total_emp_cost + total_er_cost) / request.employee_count, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8360)
