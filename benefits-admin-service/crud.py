"""CRUD operations for Benefits Administration Service.

Every query is scoped to the calling user and, when the request carries an
X-Book-ID, to that Book. Creates stamp book_id on the node; reads/writes
filter on it so cross-Book access is invisible (404).
"""

import uuid
from datetime import datetime, timezone

from benefits_admin_service.dependencies import book_id_var
from benefits_admin_service.exceptions import ConflictError, NotFoundError, ValidationError
from benefits_admin_service.models import (
    VALID_LEAVE_TYPES,
    VALID_PLAN_TYPES,
    BenefitEnrollment,
    BenefitPlan,
    LeaveAccrual,
)
from neo4j import AsyncSession


def _iso(value):
    """Coerce a Neo4j temporal (or test double) to an ISO string for Pydantic."""
    if value is None:
        return None
    if isinstance(value, str):
        return value
    if hasattr(value, "iso_format"):
        return value.iso_format()
    return str(value)


async def _run(session: AsyncSession, query: str, **params):
    merged = dict(params)
    merged["book_id"] = book_id_var.get()
    return await session.run(query, **merged)


def _plan_from(props: dict) -> BenefitPlan:
    return BenefitPlan(
        id=props["id"],
        name=props["name"],
        plan_type=props["plan_type"],
        description=props.get("description", ""),
        employer_contribution_pct=props.get("employer_contribution_pct", 0.0),
        employee_contribution_pct=props.get("employee_contribution_pct", 0.0),
        eligibility_months=props.get("eligibility_months", 0),
        status=props.get("status", "active"),
        created_at=_iso(props.get("created_at")),
    )


def _enrollment_from(props: dict) -> BenefitEnrollment:
    return BenefitEnrollment(
        id=props["id"],
        employee_id=props["employee_id"],
        plan_id=props["plan_id"],
        enrollment_date=_iso(props.get("enrollment_date")),
        status=props.get("status", "active"),
        beneficiary=props.get("beneficiary", ""),
    )


def _accrual_from(props: dict) -> LeaveAccrual:
    return LeaveAccrual(
        id=props["id"],
        employee_id=props["employee_id"],
        leave_type=props["leave_type"],
        period=props.get("period", ""),
        accrued_days=float(props.get("accrued_days", 0.0)),
        taken_days=float(props.get("taken_days", 0.0)),
        balance_days=float(props.get("balance_days", 0.0)),
        created_at=_iso(props.get("created_at")),
    )


async def create_plan(
    session: AsyncSession,
    user_id: str,
    name: str,
    plan_type: str,
    description: str = "",
    employer_contribution_pct: float = 0.0,
    employee_contribution_pct: float = 0.0,
    eligibility_months: int = 0,
) -> BenefitPlan:
    if plan_type not in VALID_PLAN_TYPES:
        raise ValidationError(detail=f"Invalid plan type. Must be one of {VALID_PLAN_TYPES}", code="INVALID_PLAN_TYPE")
    plan_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc).isoformat()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (p:BenefitPlan {
        id: $id, user_id: $user_id, book_id: $book_id,
        name: $name, plan_type: $plan_type, description: $description,
        employer_contribution_pct: toFloat($employer_contribution_pct),
        employee_contribution_pct: toFloat($employee_contribution_pct),
        eligibility_months: toInteger($eligibility_months),
        status: $status, created_at: datetime($created_at)
    })
    CREATE (u)-[:CREATED]->(p)
    RETURN p
    """
    result = await _run(
        session,
        query,
        id=plan_id,
        user_id=user_id,
        name=name,
        plan_type=plan_type,
        description=description,
        employer_contribution_pct=employer_contribution_pct,
        employee_contribution_pct=employee_contribution_pct,
        eligibility_months=eligibility_months,
        status="active",
        created_at=now,
    )
    record = await result.single()
    if not record:
        raise NotFoundError(detail="Could not create benefit plan", code="CREATE_FAILED")
    return _plan_from(record["p"])


async def list_plans(session: AsyncSession, user_id: str, plan_type: str = None) -> list:
    filters = ""
    params = {"user_id": user_id, "limit": 1000}
    if plan_type:
        filters = "AND p.plan_type = $plan_type"
        params["plan_type"] = plan_type
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:CREATED]->(p:BenefitPlan)
    WHERE ($book_id IS NULL OR p.book_id = $book_id)
      {filters}
    RETURN p
    ORDER BY p.created_at DESC
    LIMIT $limit
    """
    result = await _run(session, query, **params)
    plans = []
    async for record in result:
        plans.append(_plan_from(record["p"]))
    return plans


async def enroll_employee(
    session: AsyncSession, user_id: str, employee_id: str, plan_id: str, beneficiary: str = ""
) -> BenefitEnrollment:
    plan_query = """
    MATCH (u:User {id: $user_id})-[:CREATED]->(p:BenefitPlan {id: $plan_id})
    WHERE ($book_id IS NULL OR p.book_id = $book_id)
    RETURN p
    LIMIT $limit
    """
    plan_result = await _run(session, plan_query, user_id=user_id, plan_id=plan_id, limit=1)
    if not await plan_result.single():
        raise NotFoundError(detail="Benefit plan not found", code="PLAN_NOT_FOUND")

    dup_query = """
    MATCH (u:User {id: $user_id})-[:CREATED]->(e:BenefitEnrollment)
    WHERE ($book_id IS NULL OR e.book_id = $book_id)
      AND e.employee_id = $employee_id
      AND e.plan_id = $plan_id
      AND e.status = 'active'
    RETURN e
    LIMIT $limit
    """
    dup_result = await _run(session, dup_query, user_id=user_id, employee_id=employee_id, plan_id=plan_id, limit=1)
    if await dup_result.single():
        raise ConflictError(detail="Employee already enrolled in this plan", code="ALREADY_ENROLLED")

    enrollment_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc).isoformat()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (e:BenefitEnrollment {
        id: $id, user_id: $user_id, book_id: $book_id,
        employee_id: $employee_id, plan_id: $plan_id, beneficiary: $beneficiary,
        status: $status, enrollment_date: datetime($enrollment_date)
    })
    CREATE (u)-[:CREATED]->(e)
    RETURN e
    """
    result = await _run(
        session,
        query,
        id=enrollment_id,
        user_id=user_id,
        employee_id=employee_id,
        plan_id=plan_id,
        beneficiary=beneficiary,
        status="active",
        enrollment_date=now,
    )
    record = await result.single()
    if not record:
        raise NotFoundError(detail="Could not create enrollment", code="CREATE_FAILED")
    return _enrollment_from(record["e"])


async def list_employee_enrollments(session: AsyncSession, user_id: str, employee_id: str) -> list:
    query = """
    MATCH (u:User {id: $user_id})-[:CREATED]->(e:BenefitEnrollment)
    WHERE ($book_id IS NULL OR e.book_id = $book_id)
      AND e.employee_id = $employee_id
      AND e.status = 'active'
    RETURN e
    ORDER BY e.enrollment_date DESC
    LIMIT $limit
    """
    result = await _run(session, query, user_id=user_id, employee_id=employee_id, limit=1000)
    enrollments = []
    async for record in result:
        enrollments.append(_enrollment_from(record["e"]))
    return enrollments


async def accrue_leave(
    session: AsyncSession,
    user_id: str,
    employee_id: str,
    leave_type: str,
    period: str,
    accrued_days: float,
    taken_days: float = 0.0,
) -> LeaveAccrual:
    if leave_type not in VALID_LEAVE_TYPES:
        raise ValidationError(
            detail=f"Invalid leave type. Must be one of {VALID_LEAVE_TYPES}", code="INVALID_LEAVE_TYPE"
        )
    prev_query = """
    MATCH (u:User {id: $user_id})-[:CREATED]->(la:LeaveAccrual)
    WHERE ($book_id IS NULL OR la.book_id = $book_id)
      AND la.employee_id = $employee_id
      AND la.leave_type = $leave_type
    RETURN la
    ORDER BY la.created_at DESC
    LIMIT $limit
    """
    prev_result = await _run(
        session, prev_query, user_id=user_id, employee_id=employee_id, leave_type=leave_type, limit=1
    )
    prev = await prev_result.single()
    prev_balance = float(prev["la"].get("balance_days", 0.0)) if prev else 0.0

    balance = prev_balance + accrued_days - taken_days
    accrual_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc).isoformat()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (la:LeaveAccrual {
        id: $id, user_id: $user_id, book_id: $book_id,
        employee_id: $employee_id, leave_type: $leave_type, period: $period,
        accrued_days: toFloat($accrued_days), taken_days: toFloat($taken_days),
        balance_days: toFloat($balance_days), created_at: datetime($created_at)
    })
    CREATE (u)-[:CREATED]->(la)
    RETURN la
    """
    result = await _run(
        session,
        query,
        id=accrual_id,
        user_id=user_id,
        employee_id=employee_id,
        leave_type=leave_type,
        period=period,
        accrued_days=accrued_days,
        taken_days=taken_days,
        balance_days=balance,
        created_at=now,
    )
    record = await result.single()
    if not record:
        raise NotFoundError(detail="Could not record leave accrual", code="CREATE_FAILED")
    return _accrual_from(record["la"])


async def list_employee_leave(session: AsyncSession, user_id: str, employee_id: str, leave_type: str = None) -> list:
    filters = ""
    params = {"user_id": user_id, "employee_id": employee_id, "limit": 1000}
    if leave_type:
        filters = "AND la.leave_type = $leave_type"
        params["leave_type"] = leave_type
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:CREATED]->(la:LeaveAccrual)
    WHERE ($book_id IS NULL OR la.book_id = $book_id)
      AND la.employee_id = $employee_id
      {filters}
    RETURN la
    ORDER BY la.created_at DESC
    LIMIT $limit
    """
    result = await _run(session, query, **params)
    accruals = []
    async for record in result:
        accruals.append(_accrual_from(record["la"]))
    return accruals
