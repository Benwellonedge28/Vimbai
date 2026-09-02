"""
Vimbai Departmental Accounting Service
Dedicated service for departmental cost allocation, inter-department billing,
and department-level financial reporting
Uses existing services via internal API calls for core accounting functions
"""

import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
from fastapi import BackgroundTasks, Depends, FastAPI, HTTPException, Query
from pydantic import BaseModel, Field

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
# Enums
# ============================================================================


class DepartmentType(str, Enum):
    REVENUE = "revenue"  # Generates income
    COST = "cost"  # Incurs expenses
    SUPPORT = "support"  # Provides services to other departments
    ADMIN = "admin"  # Administrative


class AllocationMethod(str, Enum):
    DIRECT = "direct"
    STEP_DOWN = "step_down"
    RECIPROCAL = "reciprocal"
    RATIO_BASED = "ratio_based"


class AllocationBasis(str, Enum):
    HEADCOUNT = "headcount"
    FLOOR_SPACE = "floor_space"
    REVENUE = "revenue"
    EXPENSES = "expenses"
    USAGE = "usage"
    CUSTOM = "custom"


class DepartmentStatus(str, Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
    UNDER_REVIEW = "under_review"
    CLOSED = "closed"


# ============================================================================
# Pydantic Models
# ============================================================================


class Department(BaseModel):
    id: str
    department_code: str
    department_name: str
    department_type: DepartmentType
    parent_department_id: Optional[str] = None
    manager_id: str
    manager_name: str
    cost_center_code: Optional[str] = None
    revenue_center_code: Optional[str] = None
    status: DepartmentStatus = DepartmentStatus.ACTIVE
    budget_id: Optional[str] = None  # Links to budgeting-service
    account_code: Optional[str] = None  # Links to accounting-service
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DepartmentAllocationRule(BaseModel):
    id: str
    department_id: str
    cost_type: str  # rent, utilities, IT, HR, etc.
    allocation_basis: AllocationBasis
    allocation_method: AllocationMethod
    percentage: float = 0.0  # For ratio-based
    custom_formula: Optional[str] = None
    priority: int = 1  # For step-down method
    is_active: bool = True
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class InterDepartmentBilling(BaseModel):
    id: str
    bill_number: str
    from_department_id: str
    from_department_name: str
    to_department_id: str
    to_department_name: str
    service_description: str
    service_category: str
    amount: Decimal
    billing_date: datetime
    period_start: datetime
    period_end: datetime
    status: str = "pending"  # pending, approved, invoiced, paid
    approved_by: Optional[str] = None
    invoice_id: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DepartmentCostPool(BaseModel):
    id: str
    pool_name: str
    pool_type: str  # service_costs, facility_costs, admin_costs
    total_amount: Decimal
    allocation_basis: AllocationBasis
    allocation_method: AllocationMethod
    included_departments: List[str] = []
    excluded_departments: List[str] = []
    status: str = "open"  # open, allocating, closed
    period_start: datetime
    period_end: datetime
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DepartmentAllocationResult(BaseModel):
    id: str
    department_id: str
    department_name: str
    cost_pool_id: str
    cost_pool_name: str
    allocated_amount: Decimal
    allocation_percentage: float
    allocation_basis_used: AllocationBasis
    calculation_details: Dict[str, Any] = {}
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DepartmentFinancials(BaseModel):
    department_id: str
    department_name: str
    period: str
    revenue: Decimal = Decimal("0")
    direct_expenses: Decimal = Decimal("0")
    allocated_costs: Decimal = Decimal("0")
    total_expenses: Decimal = Decimal("0")
    net_income: Decimal = Decimal("0")
    budget_variance: Decimal = Decimal("0")
    budget_variance_percentage: float = 0.0
    headcount: int = 0
    revenue_per_head: Decimal = Decimal("0")
    cost_per_head: Decimal = Decimal("0")
    as_of_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class DepartmentPerformanceReport(BaseModel):
    id: str
    department_id: str
    department_name: str
    period_start: datetime
    period_end: datetime
    financial_summary: DepartmentFinancials
    kpis: Dict[str, Any] = {}
    comparisons: Dict[str, Any] = {}  # vs budget, vs previous period, vs target
    recommendations: List[str] = []
    generated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


# ============================================================================
# Storage
# ============================================================================

departments: Dict[str, Department] = {}
allocation_rules: Dict[str, DepartmentAllocationRule] = {}
inter_dept_bills: Dict[str, InterDepartmentBilling] = {}
cost_pools: Dict[str, DepartmentCostPool] = {}
allocation_results: Dict[str, List[DepartmentAllocationResult]] = {}
dept_financials_cache: Dict[str, DepartmentFinancials] = {}


# ============================================================================
# Internal API Helper Functions
# ============================================================================


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
async def health_check():
    """Health check endpoint"""
    return {
        "status": "healthy",
        "service": "departmental-accounting",
        "version": "1.0.0",
        "total_departments": len(departments),
        "active_cost_pools": sum(1 for p in cost_pools.values() if p.status == "open"),
    }


# --- Department Management ---


@app.post("/departments")
async def create_department(department: Department):
    """Create a new department"""
    department.id = str(uuid.uuid4())
    department.created_at = datetime.now(timezone.utc)
    department.updated_at = datetime.now(timezone.utc)

    departments[department.id] = department

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
):
    """List all departments"""
    results = list(departments.values())

    if department_type:
        results = [d for d in results if d.department_type == department_type]
    if status:
        results = [d for d in results if d.status == status]
    if parent_id:
        results = [d for d in results if d.parent_department_id == parent_id]

    return results


@app.get("/departments/{department_id}")
async def get_department(department_id: str):
    """Get department details"""
    if department_id not in departments:
        raise HTTPException(status_code=404, detail="Department not found")
    return departments[department_id]


@app.put("/departments/{department_id}")
async def update_department(department_id: str, department: Department):
    """Update department"""
    if department_id not in departments:
        raise HTTPException(status_code=404, detail="Department not found")

    department.id = department_id
    department.updated_at = datetime.now(timezone.utc)
    departments[department_id] = department

    return department


@app.get("/departments/{department_id}/hierarchy")
async def get_department_hierarchy(department_id: str):
    """Get department hierarchy (parent/children)"""
    if department_id not in departments:
        raise HTTPException(status_code=404, detail="Department not found")

    # Get parent chain
    parents = []
    current_id = department_id
    while current_id:
        dept = departments.get(current_id)
        if not dept:
            break
        parents.append(dept)
        current_id = dept.parent_department_id

    # Get children
    children = [d for d in departments.values() if d.parent_department_id == department_id]

    return {
        "department": departments[department_id],
        "parents": parents[1:],  # Exclude self
        "children": children,
    }


# --- Allocation Rules ---


@app.post("/allocation-rules")
async def create_allocation_rule(rule: DepartmentAllocationRule):
    """Create cost allocation rule"""
    rule.id = str(uuid.uuid4())
    rule.created_at = datetime.now(timezone.utc)

    allocation_rules[rule.id] = rule
    return rule


@app.get("/allocation-rules")
async def list_allocation_rules(department_id: Optional[str] = None):
    """List allocation rules"""
    results = list(allocation_rules.values())

    if department_id:
        results = [r for r in results if r.department_id == department_id]

    return results


@app.delete("/allocation-rules/{rule_id}")
async def deactivate_allocation_rule(rule_id: str):
    """Deactivate an allocation rule"""
    if rule_id not in allocation_rules:
        raise HTTPException(status_code=404, detail="Rule not found")

    allocation_rules[rule_id].is_active = False
    return {"status": "deactivated", "rule_id": rule_id}


# --- Cost Pools ---


@app.post("/cost-pools")
async def create_cost_pool(pool: DepartmentCostPool):
    """Create a cost pool for allocation"""
    pool.id = str(uuid.uuid4())
    pool.created_at = datetime.now(timezone.utc)

    cost_pools[pool.id] = pool
    return pool


@app.get("/cost-pools")
async def list_cost_pools(status: Optional[str] = None):
    """List cost pools"""
    results = list(cost_pools.values())

    if status:
        results = [p for p in results if p.status == status]

    return results


@app.get("/cost-pools/{pool_id}")
async def get_cost_pool(pool_id: str):
    """Get cost pool details"""
    if pool_id not in cost_pools:
        raise HTTPException(status_code=404, detail="Cost pool not found")
    return cost_pools[pool_id]


# --- Cost Allocation ---


@app.post("/allocate")
async def run_cost_allocation(
    cost_pool_id: str, period_start: datetime, period_end: datetime, created_by: str = "system"
):
    """Run cost allocation for a cost pool"""
    if cost_pool_id not in cost_pools:
        raise HTTPException(status_code=404, detail="Cost pool not found")

    pool = cost_pools[cost_pool_id]
    pool.period_start = period_start
    pool.period_end = period_end
    pool.status = "allocating"

    # Get departments to allocate to
    target_departments = [
        d for d in departments.values() if d.id not in pool.excluded_departments and d.status == DepartmentStatus.ACTIVE
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

    allocation_results[pool.id] = results
    pool.status = "closed"

    return {
        "cost_pool": pool,
        "allocations": results,
        "total_allocated": sum(r.allocated_amount for r in results),
    }


@app.get("/allocations/{pool_id}")
async def get_allocation_results(pool_id: str):
    """Get allocation results for a cost pool"""
    if pool_id not in allocation_results:
        return []
    return allocation_results[pool_id]


# --- Inter-Department Billing ---


@app.post("/inter-department-bills")
async def create_inter_dept_bill(bill: InterDepartmentBilling):
    """Create inter-department billing"""
    bill.id = str(uuid.uuid4())
    bill.created_at = datetime.now(timezone.utc)

    inter_dept_bills[bill.id] = bill

    return bill


@app.get("/inter-department-bills")
async def list_inter_dept_bills(
    from_dept_id: Optional[str] = None, to_dept_id: Optional[str] = None, status: Optional[str] = None
):
    """List inter-department bills"""
    results = list(inter_dept_bills.values())

    if from_dept_id:
        results = [b for b in results if b.from_department_id == from_dept_id]
    if to_dept_id:
        results = [b for b in results if b.to_department_id == to_dept_id]
    if status:
        results = [b for b in results if b.status == status]

    return results


@app.post("/inter-department-bills/{bill_id}/approve")
async def approve_inter_dept_bill(bill_id: str, approved_by: str):
    """Approve inter-department bill"""
    if bill_id not in inter_dept_bills:
        raise HTTPException(status_code=404, detail="Bill not found")

    bill = inter_dept_bills[bill_id]
    bill.status = "approved"
    bill.approved_by = approved_by

    # Create journal entries in accounting service
    from_dept = departments.get(bill.from_department_id)
    to_dept = departments.get(bill.to_department_id)

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


@app.get("/departments/{department_id}/financials")
async def get_department_financials(department_id: str, period_start: datetime, period_end: datetime):
    """Get department financial summary using existing services"""
    if department_id not in departments:
        raise HTTPException(status_code=404, detail="Department not found")

    dept = departments[department_id]

    # Get revenue from accounting service
    revenue_data = await call_accounting_service("GET", f"/accounts/{dept.account_code}/period-activity")
    revenue = Decimal(str(revenue_data.get("total_credits", 0)))

    # Get expenses from accounting service
    expenses = Decimal("0")
    if dept.account_code:
        ledger = await call_accounting_service("GET", f"/ledgers/{dept.account_code}")
        expenses = Decimal(str(ledger.get("closing_balance", 0)))

    # Get allocated costs
    allocated_costs = Decimal("0")
    for results in allocation_results.values():
        for result in results:
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

    # Cache the results
    cache_key = f"{dept.id}-{period_start.date()}"
    dept_financials_cache[cache_key] = financials

    return financials


@app.get("/departments/{department_id}/performance-report")
async def get_performance_report(department_id: str, period_start: datetime, period_end: datetime):
    """Generate comprehensive department performance report"""
    if department_id not in departments:
        raise HTTPException(status_code=404, detail="Department not found")

    dept = departments[department_id]

    # Get financials
    financials = await get_department_financials(department_id, period_start, period_end)

    # Get budget execution from budgeting service
    budget_execution = {}
    if dept.budget_id:
        budget_execution = await call_budgeting_service("GET", f"/reports/execution?budget_id={dept.budget_id}")

    # Calculate KPIs
    kpis = {
        "profit_margin": float(financials.net_income / financials.revenue * 100) if financials.revenue else 0,
        "cost_ratio": float(financials.total_expenses / financials.revenue * 100) if financials.revenue else 0,
        "cost_efficiency": (
            "Good" if float(financials.total_expenses / financials.revenue * 100) < 80 else "Needs Improvement"
        ),
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
async def compare_departments(department_ids: List[str], period_start: datetime, period_end: datetime):
    """Compare financial performance across departments"""
    comparisons = []

    for dept_id in department_ids:
        financials = await get_department_financials(dept_id, period_start, period_end)
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
async def get_cost_distribution_report(period_start: datetime, period_end: datetime):
    """Get cost distribution across all departments"""
    distribution = []

    for dept in departments.values():
        if dept.status == DepartmentStatus.ACTIVE:
            financials = await get_department_financials(dept.id, period_start, period_end)

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
