"""Vimbai Departmental Accounting Service. Port: 8100.

Departmental cost allocation, inter-department billing, and
department-level financial reporting. The six module-level stores
previously shared ALL callers' data globally; they now persist to Neo4j
as caller-owned, Book-scoped records (X-User-Id / X-Book-ID). Rerunning
a cost allocation replaces the caller's previous results for that pool.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "departmental_accounting_service" not in _sys.modules or not hasattr(
    _sys.modules.get("departmental_accounting_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "departmental_accounting_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["departmental_accounting_service"] = _pkg
    _sys.modules["departmental_accounting_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

import httpx
from fastapi import Depends, FastAPI, HTTPException, Request
from neo4j import AsyncSession

from departmental_accounting_service import crud
from departmental_accounting_service.dependencies import book_id_var, get_db_session, get_user_id
from departmental_accounting_service.exceptions import DepartmentalAccountingError
from departmental_accounting_service.models import (
    AllocationBasis,
    AllocationMethod,
    Department,
    DepartmentAllocationResult,
    DepartmentAllocationRule,
    DepartmentCostPool,
    DepartmentFinancials,
    DepartmentPerformanceReport,
    DepartmentStatus,
    DepartmentType,
    InterDepartmentBilling,
)

app = FastAPI(
    title="Vimbai Departmental Accounting Service",
    description="Departmental cost allocation, inter-department billing, and department-level financial reporting using existing Vimbai services",
    version="1.0.0",
)

# ============================================================================
# Configuration - Internal API endpoints
# ============================================================================

ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")
BUDGETING_SERVICE_URL = os.getenv("BUDGETING_SERVICE_URL", "http://localhost:8099")
CASHBOOK_SERVICE_URL = os.getenv("CASHBOOK_SERVICE_URL", "http://localhost:8098")
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8091")

# ============================================================================
# Internal API Helper Functions
# ============================================================================


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(DepartmentalAccountingError)
async def _departmental_accounting_error(request: Request, exc: DepartmentalAccountingError):
    from fastapi.responses import JSONResponse

    status_code = getattr(exc, "status_code", 400)
    return JSONResponse(
        status_code=status_code, content={"detail": str(exc), "error": exc.__class__.__name__}
    )


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None):
    """Call accounting service for core accounting functions"""
    async with httpx.AsyncClient() as client:
        url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
        try:
            if method == "GET":
                response = await client.get(url, timeout=10.0)
            elif method == "POST":
                response = await client.post(url, json=data, timeout=10.0)
            else:
                return {"error": "Method not supported"}
            return response.json()
        except httpx.RequestError:
            return {"error": "Accounting service unavailable", "data": None}


async def call_budgeting_service(method: str, endpoint: str, data: Optional[Dict] = None):
    """Call budgeting service for budget functions"""
    async with httpx.AsyncClient() as client:
        url = f"{BUDGETING_SERVICE_URL}{endpoint}"
        try:
            if method == "GET":
                response = await client.get(url, timeout=10.0)
            elif method == "POST":
                response = await client.post(url, json=data, timeout=10.0)
            else:
                return {"error": "Method not supported"}
            return response.json()
        except httpx.RequestError:
            return {"error": "Budgeting service unavailable", "data": None}


async def call_cashbook_service(method: str, endpoint: str, data: Optional[Dict] = None):
    """Call cashbook service for cash functions"""
    async with httpx.AsyncClient() as client:
        url = f"{CASHBOOK_SERVICE_URL}{endpoint}"
        try:
            if method == "GET":
                response = await client.get(url, timeout=10.0)
            elif method == "POST":
                response = await client.post(url, json=data, timeout=10.0)
            else:
                return {"error": "Method not supported"}
            return response.json()
        except httpx.RequestError:
            return {"error": "Cashbook service unavailable", "data": None}


async def call_audit_service(event_data: Dict):
    """Log to audit service"""
    async with httpx.AsyncClient() as client:
        url = f"{AUDIT_SERVICE_URL}/events"
        try:
            await client.post(url, json=event_data, timeout=5.0)
        except httpx.RequestError:
            pass  # Don't fail if audit is unavailable


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Health check endpoint (caller-scoped counts)"""
    departments = await crud.list_all(db_session, user_id, Department)
    cost_pools = await crud.list_all(db_session, user_id, DepartmentCostPool)
    return {
        "status": "healthy",
        "service": "departmental-accounting",
        "version": "1.0.0",
        "total_departments": len(departments),
        "active_cost_pools": sum(1 for p in cost_pools if p.status == "open"),
    }


# --- Department Management ---


@app.post("/departments")
async def create_department(
    department: Department,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new department (caller-owned, Book-stamped)"""
    department.id = str(uuid.uuid4())
    department.created_at = datetime.now(timezone.utc)
    department.updated_at = datetime.now(timezone.utc)

    await crud.create(db_session, user_id, department)

    # Create cost center in accounting service
    await call_accounting_service(
        "POST",
        "/accounts/",
        {
            "account_number": f"DEPT-{department.department_code}",
            "account_name": f"{department.department_name} - Cost Center",
            "account_type": "Expense",
            "description": f"Cost center for {department.department_name}",
        },
    )

    # Log to audit
    await call_audit_service(
        {
            "event_type": "create",
            "resource_type": "department",
            "resource_id": department.id,
            "user_id": "system",
            "action_details": {"department_name": department.department_name},
        }
    )

    return department


@app.get("/departments")
async def list_departments(
    department_type: Optional[DepartmentType] = None,
    status: Optional[DepartmentStatus] = None,
    parent_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's departments"""
    results = await crud.list_all(db_session, user_id, Department)

    if department_type:
        results = [d for d in results if d.department_type == department_type]
    if status:
        results = [d for d in results if d.status == status]
    if parent_id:
        results = [d for d in results if d.parent_department_id == parent_id]

    return results


@app.get("/departments/{department_id}")
async def get_department(
    department_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get department details; cross-scope 404"""
    dept = await crud.find(db_session, user_id, Department, department_id)
    if not dept:
        raise HTTPException(status_code=404, detail="Department not found")
    return dept


@app.put("/departments/{department_id}")
async def update_department(
    department_id: str,
    department: Department,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update department (full replace, caller-owned)"""
    existing = await crud.find(db_session, user_id, Department, department_id)
    if not existing:
        raise HTTPException(status_code=404, detail="Department not found")

    department.id = department_id
    department.updated_at = datetime.now(timezone.utc)

    # latest-wins upsert: replace the caller's record for this id
    await crud.delete_where(db_session, user_id, Department, {"id": department_id})
    await crud.create(db_session, user_id, department)

    return department


@app.get("/departments/{department_id}/hierarchy")
async def get_department_hierarchy(
    department_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get department hierarchy (parent/children) within the caller's scope"""
    dept = await crud.find(db_session, user_id, Department, department_id)
    if not dept:
        raise HTTPException(status_code=404, detail="Department not found")

    all_departments = await crud.list_all(db_session, user_id, Department)
    by_id = {d.id: d for d in all_departments}

    # Get parent chain
    parents = []
    current_id = department_id
    while current_id:
        d = by_id.get(current_id)
        if not d:
            break
        parents.append(d)
        current_id = d.parent_department_id

    # Get children
    children = [d for d in all_departments if d.parent_department_id == department_id]

    return {
        "department": dept,
        "parents": parents[1:],  # Exclude self
        "children": children,
    }


# --- Allocation Rules ---


@app.post("/allocation-rules")
async def create_allocation_rule(
    rule: DepartmentAllocationRule,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create cost allocation rule (caller-owned)"""
    rule.id = str(uuid.uuid4())
    rule.created_at = datetime.now(timezone.utc)

    await crud.create(db_session, user_id, rule)
    return rule


@app.get("/allocation-rules")
async def list_allocation_rules(
    department_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's allocation rules"""
    results = await crud.list_all(db_session, user_id, DepartmentAllocationRule)

    if department_id:
        results = [r for r in results if r.department_id == department_id]

    return results


@app.delete("/allocation-rules/{rule_id}")
async def deactivate_allocation_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Deactivate an allocation rule"""
    updated = await crud.update_props(
        db_session, user_id, DepartmentAllocationRule, rule_id, {"is_active": False}
    )
    if not updated:
        raise HTTPException(status_code=404, detail="Rule not found")

    return {"status": "deactivated", "rule_id": rule_id}


# --- Cost Pools ---


@app.post("/cost-pools")
async def create_cost_pool(
    pool: DepartmentCostPool,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a cost pool for allocation (caller-owned)"""
    pool.id = str(uuid.uuid4())
    pool.created_at = datetime.now(timezone.utc)

    await crud.create(db_session, user_id, pool)
    return pool


@app.get("/cost-pools")
async def list_cost_pools(
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's cost pools"""
    results = await crud.list_all(db_session, user_id, DepartmentCostPool)

    if status:
        results = [p for p in results if p.status == status]

    return results


@app.get("/cost-pools/{pool_id}")
async def get_cost_pool(
    pool_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cost pool details; cross-scope 404"""
    pool = await crud.find(db_session, user_id, DepartmentCostPool, pool_id)
    if not pool:
        raise HTTPException(status_code=404, detail="Cost pool not found")
    return pool


# --- Cost Allocation ---


@app.post("/allocate")
async def run_cost_allocation(
    cost_pool_id: str,
    period_start: datetime,
    period_end: datetime,
    created_by: str = "system",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Run cost allocation for a caller-owned cost pool"""
    pool = await crud.find(db_session, user_id, DepartmentCostPool, cost_pool_id)
    if not pool:
        raise HTTPException(status_code=404, detail="Cost pool not found")

    pool.period_start = period_start
    pool.period_end = period_end
    pool.status = "allocating"
    await crud.update_props(
        db_session,
        user_id,
        DepartmentCostPool,
        cost_pool_id,
        {
            "period_start": period_start,
            "period_end": period_end,
            "status": "allocating",
        },
    )

    # Get departments to allocate to (caller's own, active, not excluded)
    all_departments = await crud.list_all(db_session, user_id, Department)
    target_departments = [
        d
        for d in all_departments
        if d.id not in pool.excluded_departments and d.status == DepartmentStatus.ACTIVE
    ]

    results = []

    # Get cost center expenses from accounting service
    for dept in target_departments:
        if dept.account_code:
            # Call accounting service for ledger data
            ledger_data = await call_accounting_service("GET", f"/ledgers/{dept.account_code}")
            total_expenses = Decimal(str(ledger_data.get("closing_balance", 0)))
        else:
            total_expenses = Decimal("0")

        # Calculate allocation based on basis
        if pool.allocation_basis == AllocationBasis.HEADCOUNT:
            # Use headcount-based allocation (would integrate with HR service)
            allocation_percentage = 100.0 / len(target_departments)
        elif pool.allocation_basis == AllocationBasis.REVENUE:
            # Call accounting service for department revenue
            revenue_data = await call_accounting_service("GET", f"/accounts/{dept.account_code}/period-activity")
            revenue = Decimal(str(revenue_data.get("total_credits", 0)))
            total_revenue = sum(
                Decimal(str(r.get("total_credits", 0)))
                for r in [
                    await call_accounting_service("GET", f"/accounts/{d.account_code}/period-activity")
                    for d in target_departments
                ]
            )
            allocation_percentage = float(revenue / total_revenue * 100) if total_revenue else 0
        elif pool.allocation_basis == AllocationBasis.EXPENSES:
            allocation_percentage = (
                float(total_expenses / sum(total_expenses for _ in target_departments) * 100) if total_expenses else 0
            )
        else:
            allocation_percentage = 100.0 / len(target_departments)

        allocated_amount = pool.total_amount * Decimal(str(allocation_percentage / 100))

        result = DepartmentAllocationResult(
            id=str(uuid.uuid4()),
            department_id=dept.id,
            department_name=dept.department_name,
            cost_pool_id=pool.id,
            cost_pool_name=pool.pool_name,
            allocated_amount=allocated_amount,
            allocation_percentage=allocation_percentage,
            allocation_basis_used=pool.allocation_basis,
            calculation_details={
                "basis": pool.allocation_basis.value,
                "method": pool.allocation_method.value,
                "total_pool": str(pool.total_amount),
            },
            created_at=datetime.now(timezone.utc),
        )
        results.append(result)

        # Create journal entry in accounting service to record allocation
        await call_accounting_service(
            "POST",
            "/journal-entries/",
            {
                "description": f"Cost allocation from {pool.pool_name} to {dept.department_name}",
                "reference": f"ALLOC-{pool.id[:8]}",
                "date": datetime.now().isoformat(),
                "lines": [
                    {
                        "account_code": f"DEPT-{dept.department_code}",
                        "description": f"Allocated cost from {pool.pool_name}",
                        "debit": True,
                        "amount": str(allocated_amount),
                    },
                    {
                        "account_code": f"POOL-{pool.id[:8]}",
                        "description": f"Cost pool {pool.pool_name}",
                        "debit": False,
                        "amount": str(allocated_amount),
                    },
                ],
            },
        )

    # latest-wins: replace the caller's prior results for this pool
    await crud.delete_where(
        db_session, user_id, DepartmentAllocationResult, {"cost_pool_id": pool.id}
    )
    for result in results:
        await crud.create(db_session, user_id, result)
    pool.status = "closed"
    await crud.update_props(db_session, user_id, DepartmentCostPool, cost_pool_id, {"status": "closed"})

    return {
        "cost_pool": pool,
        "allocations": results,
        "total_allocated": sum(r.allocated_amount for r in results),
    }


@app.get("/allocations/{pool_id}")
async def get_allocation_results(
    pool_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's allocation results for a cost pool"""
    results = [
        r
        for r in await crud.list_all(db_session, user_id, DepartmentAllocationResult)
        if r.cost_pool_id == pool_id
    ]
    return results


# --- Inter-Department Billing ---


@app.post("/inter-department-bills")
async def create_inter_dept_bill(
    bill: InterDepartmentBilling,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create inter-department billing (caller-owned)"""
    bill.id = str(uuid.uuid4())
    bill.created_at = datetime.now(timezone.utc)

    await crud.create(db_session, user_id, bill)

    return bill


@app.get("/inter-department-bills")
async def list_inter_dept_bills(
    from_dept_id: Optional[str] = None,
    to_dept_id: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's inter-department bills"""
    results = await crud.list_all(db_session, user_id, InterDepartmentBilling)

    if from_dept_id:
        results = [b for b in results if b.from_department_id == from_dept_id]
    if to_dept_id:
        results = [b for b in results if b.to_department_id == to_dept_id]
    if status:
        results = [b for b in results if b.status == status]

    return results


@app.post("/inter-department-bills/{bill_id}/approve")
async def approve_inter_dept_bill(
    bill_id: str,
    approved_by: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Approve inter-department bill; cross-scope 404"""
    bill = await crud.find(db_session, user_id, InterDepartmentBilling, bill_id)
    if not bill:
        raise HTTPException(status_code=404, detail="Bill not found")

    bill.status = "approved"
    bill.approved_by = approved_by
    await crud.update_props(
        db_session,
        user_id,
        InterDepartmentBilling,
        bill_id,
        {"status": "approved", "approved_by": approved_by},
    )

    # Create journal entries in accounting service
    all_departments = await crud.list_all(db_session, user_id, Department)
    from_dept = next((d for d in all_departments if d.id == bill.from_department_id), None)
    to_dept = next((d for d in all_departments if d.id == bill.to_department_id), None)

    if from_dept and to_dept:
        await call_accounting_service(
            "POST",
            "/journal-entries/",
            {
                "description": f"Inter-dept billing: {from_dept.department_name} -> {to_dept.department_name}",
                "reference": bill.bill_number,
                "date": bill.billing_date.isoformat(),
                "lines": [
                    {
                        "account_code": from_dept.cost_center_code or f"DEPT-{from_dept.department_code}",
                        "description": f"Charge to {to_dept.department_name}",
                        "debit": True,
                        "amount": str(bill.amount),
                    },
                    {
                        "account_code": to_dept.revenue_center_code or f"DEPT-{to_dept.department_code}",
                        "description": f"Service provided to {from_dept.department_name}",
                        "debit": False,
                        "amount": str(bill.amount),
                    },
                ],
            },
        )

    return bill


# --- Department Financials ---


async def _department_financials(
    user_id: str,
    db_session: AsyncSession,
    department_id: str,
    period_start: datetime,
    period_end: datetime,
) -> DepartmentFinancials:
    """Compute a department financial summary over the caller's own data."""
    dept = await crud.find(db_session, user_id, Department, department_id)
    if not dept:
        raise HTTPException(status_code=404, detail="Department not found")

    # Get revenue from accounting service
    revenue_data = await call_accounting_service("GET", f"/accounts/{dept.account_code}/period-activity")
    revenue = Decimal(str(revenue_data.get("total_credits", 0)))

    # Get expenses from accounting service
    expenses = Decimal("0")
    if dept.account_code:
        ledger = await call_accounting_service("GET", f"/ledgers/{dept.account_code}")
        expenses = Decimal(str(ledger.get("closing_balance", 0)))

    # Get allocated costs (caller's own allocation results)
    allocated_costs = Decimal("0")
    for result in await crud.list_all(db_session, user_id, DepartmentAllocationResult):
        if result.department_id == department_id:
            allocated_costs += result.allocated_amount

    # Get budget variance from budgeting service
    budget_variance = Decimal("0")
    budget_variance_pct = 0.0
    if dept.budget_id:
        budget_data = await call_budgeting_service("GET", f"/reports/budget-summary?budget_id={dept.budget_id}")
        budget_total = Decimal(str(budget_data.get("total_amount", 0)))
        if budget_total > 0:
            budget_variance = budget_total - revenue
            budget_variance_pct = float(budget_variance / budget_total * 100)

    total_expenses = expenses + allocated_costs
    net_income = revenue - total_expenses

    financials = DepartmentFinancials(
        department_id=dept.id,
        department_name=dept.department_name,
        period=f"{period_start.date()} to {period_end.date()}",
        revenue=revenue,
        direct_expenses=expenses,
        allocated_costs=allocated_costs,
        total_expenses=total_expenses,
        net_income=net_income,
        budget_variance=budget_variance,
        budget_variance_percentage=budget_variance_pct,
    )

    # Cache the results (latest-wins per department+period, caller-owned)
    cache_key = f"{dept.id}-{period_start.date()}"
    await crud.delete_where(db_session, user_id, DepartmentFinancials, {"cache_key": cache_key})
    await crud.create(db_session, user_id, financials, extra={"cache_key": cache_key})

    return financials


@app.get("/departments/{department_id}/financials")
async def get_department_financials(
    department_id: str,
    period_start: datetime,
    period_end: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get department financial summary using existing services"""
    return await _department_financials(user_id, db_session, department_id, period_start, period_end)


@app.get("/departments/{department_id}/performance-report")
async def get_performance_report(
    department_id: str,
    period_start: datetime,
    period_end: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Generate comprehensive department performance report"""
    dept = await crud.find(db_session, user_id, Department, department_id)
    if not dept:
        raise HTTPException(status_code=404, detail="Department not found")

    # Get financials
    financials = await _department_financials(user_id, db_session, department_id, period_start, period_end)

    # Get budget execution from budgeting service
    budget_execution = {}
    if dept.budget_id:
        budget_execution = await call_budgeting_service("GET", f"/reports/execution?budget_id={dept.budget_id}")

    # Calculate KPIs
    cost_ratio_pct = float(financials.total_expenses / financials.revenue * 100) if financials.revenue else 0
    kpis = {
        "profit_margin": float(financials.net_income / financials.revenue * 100) if financials.revenue else 0,
        "cost_ratio": cost_ratio_pct,
        # zero-revenue departments: original divided by zero here (latent 500) - guard like the siblings
        "cost_efficiency": "Good" if cost_ratio_pct < 80 else "Needs Improvement",
        "budget_adherence": "On Budget" if abs(financials.budget_variance_percentage) < 5 else "Over/Under Budget",
    }

    # Generate recommendations
    recommendations = []
    if kpis["profit_margin"] < 10:
        recommendations.append("Consider increasing prices or reducing costs to improve profit margin")
    if kpis["cost_ratio"] > 90:
        recommendations.append("Review expense allocations and identify cost reduction opportunities")
    if financials.allocated_costs > financials.direct_expenses * Decimal("0.5"):
        recommendations.append("High allocated costs relative to direct expenses - review allocation methods")

    report = DepartmentPerformanceReport(
        id=str(uuid.uuid4()),
        department_id=dept.id,
        department_name=dept.department_name,
        period_start=period_start,
        period_end=period_end,
        financial_summary=financials,
        kpis=kpis,
        comparisons={"budget_execution": budget_execution},
        recommendations=recommendations,
        generated_at=datetime.now(timezone.utc),
    )

    return report


# --- Reports ---


@app.get("/reports/department-comparison")
async def compare_departments(
    department_ids: List[str],
    period_start: datetime,
    period_end: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Compare financial performance across the caller's departments"""
    comparisons = []

    for dept_id in department_ids:
        financials = await _department_financials(user_id, db_session, dept_id, period_start, period_end)
        comparisons.append(financials)

    # Sort by net income
    comparisons.sort(key=lambda x: x.net_income, reverse=True)

    return {
        "period": f"{period_start.date()} to {period_end.date()}",
        "departments": comparisons,
        "summary": {
            "total_revenue": str(sum(c.revenue for c in comparisons)),
            "total_expenses": str(sum(c.total_expenses for c in comparisons)),
            "total_net_income": str(sum(c.net_income for c in comparisons)),
        },
    }


@app.get("/reports/cost-distribution")
async def get_cost_distribution_report(
    period_start: datetime,
    period_end: datetime,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get cost distribution across the caller's active departments"""
    distribution = []

    for dept in await crud.list_all(db_session, user_id, Department):
        if dept.status != DepartmentStatus.ACTIVE:
            continue
        financials = await _department_financials(user_id, db_session, dept.id, period_start, period_end)

        distribution.append(
            {
                "department_id": dept.id,
                "department_name": dept.department_name,
                "department_type": dept.department_type.value,
                "direct_costs": str(financials.direct_expenses),
                "allocated_costs": str(financials.allocated_costs),
                "total_costs": str(financials.total_expenses),
                "percentage_of_total": 0,  # Calculate after
            }
        )

    total_costs = sum(Decimal(str(d["total_costs"])) for d in distribution)
    for d in distribution:
        d["percentage_of_total"] = float(Decimal(d["total_costs"]) / total_costs * 100) if total_costs else 0

    return {
        "period": f"{period_start.date()} to {period_end.date()}",
        "distribution": distribution,
        "total_cost_pool": str(total_costs),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8100)
