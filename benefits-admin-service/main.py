"""
Vimbai Benefits Administration Service
Manages employee benefits: pension, medical, and leave accruals.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "benefits-admin-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8360"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Benefits Administration Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class BenefitPlan(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    plan_type: str  # pension, medical, dental, life_insurance, leave
    description: str = ""
    employer_contribution_pct: float = 0.0
    employee_contribution_pct: float = 0.0
    eligibility_months: int = 0
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BenefitEnrollment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    employee_id: str
    plan_id: str
    enrollment_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    status: str = "active"  # active, opted_out, terminated
    beneficiary: str = ""


class LeaveAccrual(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    employee_id: str
    leave_type: str  # annual, sick, maternity, compassionate
    period: str  # YYYY-MM
    accrued_days: float
    taken_days: float
    balance_days: float
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


plans: List[BenefitPlan] = []
enrollments: List[BenefitEnrollment] = []
leave_accruals: List[LeaveAccrual] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/plans", response_model=BenefitPlan)
async def create_plan(
    name: str,
    plan_type: str,
    description: str = "",
    employer_contribution_pct: float = 0.0,
    employee_contribution_pct: float = 0.0,
    eligibility_months: int = 0,
):
    """Create a benefit plan."""
    valid_types = ["pension", "medical", "dental", "life_insurance", "leave"]
    if plan_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid plan type. Must be one of {valid_types}")

    plan = BenefitPlan(
        name=name,
        plan_type=plan_type,
        description=description,
        employer_contribution_pct=employer_contribution_pct,
        employee_contribution_pct=employee_contribution_pct,
        eligibility_months=eligibility_months,
    )
    plans.append(plan)
    logger.info("Benefit plan created", plan_id=plan.id, name=name, type=plan_type)
    return plan


@app.get("/plans", response_model=List[BenefitPlan])
async def list_plans(plan_type: Optional[str] = None):
    """List benefit plans."""
    if plan_type:
        return [p for p in plans if p.plan_type == plan_type]
    return plans


@app.post("/enroll", response_model=BenefitEnrollment)
async def enroll_employee(employee_id: str, plan_id: str, beneficiary: str = ""):
    """Enroll an employee in a benefit plan."""
    plan = next((p for p in plans if p.id == plan_id), None)
    if not plan:
        raise HTTPException(status_code=404, detail="Benefit plan not found")

    existing = next(
        (e for e in enrollments if e.employee_id == employee_id and e.plan_id == plan_id and e.status == "active"),
        None,
    )
    if existing:
        raise HTTPException(status_code=409, detail="Employee already enrolled in this plan")

    enrollment = BenefitEnrollment(
        employee_id=employee_id,
        plan_id=plan_id,
        beneficiary=beneficiary,
    )
    enrollments.append(enrollment)
    logger.info("Employee enrolled", enrollment_id=enrollment.id, employee_id=employee_id, plan_id=plan_id)
    return enrollment


@app.get("/employee/{employee_id}/enrollments", response_model=List[BenefitEnrollment])
async def get_employee_enrollments(employee_id: str):
    """Get benefit enrollments for an employee."""
    return [e for e in enrollments if e.employee_id == employee_id and e.status == "active"]


@app.post("/leave/accrue", response_model=LeaveAccrual)
async def accrue_leave(employee_id: str, leave_type: str, period: str, accrued_days: float, taken_days: float = 0.0):
    """Record leave accrual for an employee."""
    valid_types = ["annual", "sick", "maternity", "compassionate"]
    if leave_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid leave type. Must be one of {valid_types}")

    # Find previous balance
    prev = [la for la in leave_accruals if la.employee_id == employee_id and la.leave_type == leave_type]
    prev_balance = prev[-1].balance_days if prev else 0.0

    balance = prev_balance + accrued_days - taken_days
    accrual = LeaveAccrual(
        employee_id=employee_id,
        leave_type=leave_type,
        period=period,
        accrued_days=accrued_days,
        taken_days=taken_days,
        balance_days=balance,
    )
    leave_accruals.append(accrual)
    logger.info("Leave accrued", employee_id=employee_id, type=leave_type, balance=balance)
    return accrual


@app.get("/employee/{employee_id}/leave", response_model=List[LeaveAccrual])
async def get_leave_balance(employee_id: str, leave_type: Optional[str] = None):
    """Get leave balances for an employee."""
    result = [la for la in leave_accruals if la.employee_id == employee_id]
    if leave_type:
        result = [la for la in result if la.leave_type == leave_type]
    return result


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
