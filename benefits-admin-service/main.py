"""
Vimbai Benefits Administration Service
Manages employee benefits: pension, medical, and leave accruals.
"""

# This file may be imported bare (bracket mounts, uvicorn main:app), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "benefits_admin_service" not in _sys.modules or not hasattr(_sys.modules.get("benefits_admin_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("benefits_admin_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules["benefits_admin_service"] = _pkg

import os
from typing import Optional

import structlog
from benefits_admin_service import crud
from benefits_admin_service.database import Neo4jConnector
from benefits_admin_service.dependencies import book_id_var, get_db_session, get_user_id
from benefits_admin_service.exceptions import BenefitsError
from dotenv import load_dotenv
from fastapi import Depends, FastAPI, Query, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

load_dotenv()

SERVICE_NAME = "benefits-admin-service"
SERVICE_VERSION = "2.0.0"
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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Bind the gateway-verified X-Book-ID into the request-scoped contextvar."""
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


@app.exception_handler(BenefitsError)
async def benefits_error_handler(request: Request, exc: BenefitsError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=exc.status_code,
        content={"detail": exc.detail, "code": exc.code},
    )


@app.on_event("startup")
async def startup():
    try:
        await Neo4jConnector.verify_connectivity()
        logger.info("Neo4j connectivity verified")
    except Exception:  # pragma: no cover - dev mode without database
        logger.warning("Neo4j not reachable; running without connectivity check")


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/plans", response_model=crud.BenefitPlan)
async def create_plan(
    name: str,
    plan_type: str,
    description: str = "",
    employer_contribution_pct: float = 0.0,
    employee_contribution_pct: float = 0.0,
    eligibility_months: int = 0,
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    """Create a benefit plan."""
    plan = await crud.create_plan(
        session,
        user_id,
        name,
        plan_type,
        description,
        employer_contribution_pct,
        employee_contribution_pct,
        eligibility_months,
    )
    logger.info("Benefit plan created", plan_id=plan.id, name=name, type=plan_type)
    return plan


@app.get("/plans", response_model=list[crud.BenefitPlan])
async def list_plans(
    plan_type: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    """List benefit plans."""
    return await crud.list_plans(session, user_id, plan_type)


@app.post("/enroll", response_model=crud.BenefitEnrollment)
async def enroll_employee(
    employee_id: str,
    plan_id: str,
    beneficiary: str = "",
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    """Enroll an employee in a benefit plan."""
    enrollment = await crud.enroll_employee(session, user_id, employee_id, plan_id, beneficiary)
    logger.info("Employee enrolled", enrollment_id=enrollment.id, employee_id=employee_id, plan_id=plan_id)
    return enrollment


@app.get("/employee/{employee_id}/enrollments", response_model=list[crud.BenefitEnrollment])
async def get_employee_enrollments(
    employee_id: str,
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    """Get benefit enrollments for an employee."""
    return await crud.list_employee_enrollments(session, user_id, employee_id)


@app.post("/leave/accrue", response_model=crud.LeaveAccrual)
async def accrue_leave(
    employee_id: str,
    leave_type: str,
    period: str,
    accrued_days: float = Query(..., gt=0),
    taken_days: float = 0.0,
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    """Record leave accrual for an employee."""
    accrual = await crud.accrue_leave(session, user_id, employee_id, leave_type, period, accrued_days, taken_days)
    logger.info("Leave accrued", employee_id=employee_id, type=leave_type, balance=accrual.balance_days)
    return accrual


@app.get("/employee/{employee_id}/leave", response_model=list[crud.LeaveAccrual])
async def get_leave_balance(
    employee_id: str,
    leave_type: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    session: AsyncSession = Depends(get_db_session),
):
    """Get leave balances for an employee."""
    return await crud.list_employee_leave(session, user_id, employee_id, leave_type)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
