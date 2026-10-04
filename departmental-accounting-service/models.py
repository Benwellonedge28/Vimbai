"""Pydantic models and enums for Departmental Accounting Service."""

from datetime import datetime, timezone
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


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


class DepartmentComparisonRequest(BaseModel):
    """Body for POST /reports/department-comparison"""

    department_ids: List[str]
    period_start: datetime
    period_end: datetime
