"""
FinAcc Budgeting Service
Standalone, modular budgeting service that can be used flexibly across the platform
Supports multiple budget types, departments, projects, scenarios, and variance analysis
Can integrate with any module for comprehensive financial planning
"""

from fastapi import FastAPI, HTTPException, Depends, BackgroundTasks, Query
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone, timedelta
from enum import Enum
import uuid
from decimal import Decimal

app = FastAPI(
    title="FinAcc Budgeting Service",
    description="Comprehensive, modular budgeting service for financial planning, variance analysis, and multi-scenario forecasting",
    version="1.0.0",
)

# ============================================================================
# Enums
# ============================================================================

class BudgetType(str, Enum):
    OPERATING = "operating"
    CAPITAL = "capital"
    CASH = "cash"
    MASTER = "master"
    DEPARTMENT = "department"
    PROJECT = "project"
    PROGRAM = "program"
    FUND = "fund"


class BudgetStatus(str, Enum):
    DRAFT = "draft"
    SUBMITTED = "submitted"
    REVIEW = "review"
    APPROVED = "approved"
    REJECTED = "rejected"
    REVISED = "revised"
    CLOSED = "closed"


class BudgetPeriod(str, Enum):
    MONTHLY = "monthly"
    QUARTERLY = "quarterly"
    HALF_YEARLY = "half_yearly"
    ANNUAL = "annual"


class VarianceLevel(str, Enum):
    GREEN = "green"  # Within 5%
    YELLOW = "yellow"  # 5-15%
    RED = "red"  # Over 15%


class ScenarioType(str, Enum):
    BASE = "base"
    OPTIMISTIC = "optimistic"
    PESSIMISTIC = "pessimistic"
    BEST_CASE = "best_case"
    WORST_CASE = "worst_case"
    CUSTOM = "custom"


# ============================================================================
# Pydantic Models
# ============================================================================

class Budget(BaseModel):
    id: str
    budget_code: str
    name: str
    description: Optional[str] = None
    budget_type: BudgetType
    period_type: BudgetPeriod
    fiscal_year: str  # e.g., "2024"
    start_date: datetime
    end_date: datetime
    total_amount: Decimal
    currency: str = "USD"
    status: BudgetStatus = BudgetStatus.DRAFT
    owner_id: str
    owner_name: str
    department_id: Optional[str] = None
    department_name: Optional[str] = None
    project_id: Optional[str] = None
    accounting_standard: Optional[str] = None
    approval_workflow_id: Optional[str] = None
    approved_by: Optional[str] = None
    approved_at: Optional[datetime] = None
    version: int = 1
    parent_budget_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BudgetLine(BaseModel):
    id: str
    budget_id: str
    account_code: str
    account_name: str
    category: str  # Revenue, COGS, OpEx, etc.
    subcategory: Optional[str] = None
    description: str
    annual_amount: Decimal
    allocated_amount: Decimal
    remaining_amount: Decimal
    period_allocation: Dict[str, Decimal] = {}  # month -> amount
    actual_amount: Decimal = Decimal("0")
    variance: Decimal = Decimal("0")
    variance_percentage: float = 0.0
    notes: Optional[str] = None


class BudgetAllocation(BaseModel):
    id: str
    budget_id: str
    allocation_type: str  # department, project, cost_center
    allocated_to: str  # department_id or project_id
    allocated_to_name: str
    amount: Decimal
    percentage: float
    start_date: Optional[datetime] = None
    end_date: Optional[datetime] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BudgetRevision(BaseModel):
    id: str
    budget_id: str
    revision_number: int
    revision_date: datetime
    revised_by: str
    reason: str
    changes: List[Dict[str, Any]] = []  # Details of changes made
    previous_total: Decimal
    new_total: Decimal
    approved_by: Optional[str] = None
    status: str = "pending"  # pending, approved, rejected


class BudgetScenario(BaseModel):
    id: str
    name: str
    description: str
    scenario_type: ScenarioType
    budget_id: str  # Base budget
    growth_rate: float = 0.0  # % growth applied
    inflation_rate: float = 0.0
    assumption_notes: str
    modified_lines: Dict[str, Decimal] = {}  # account_code -> new_amount
    created_by: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BudgetForecast(BaseModel):
    id: str
    budget_id: str
    forecast_period: str  # e.g., "2024-Q1"
    forecast_type: str  # rolling, seasonal, annual
    forecast_data: List[Dict[str, Any]] = []  # period data
    confidence_level: float
    methodology: str  # time series, causal, judgment
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BudgetApproval(BaseModel):
    id: str
    budget_id: str
    approval_level: int
    approver_id: str
    approver_name: str
    status: str  # pending, approved, rejected, returned
    decision_date: Optional[datetime] = None
    comments: Optional[str] = None
    sequence: int  # Order in approval chain


class VarianceAnalysis(BaseModel):
    id: str
    budget_id: str
    account_code: str
    account_name: str
    period: str
    budgeted_amount: Decimal
    actual_amount: Decimal
    variance: Decimal
    variance_percentage: float
    variance_level: VarianceLevel
    explanation: Optional[str] = None
    recommended_action: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BudgetConsolidation(BaseModel):
    id: str
    consolidation_name: str
    fiscal_year: str
    period_type: BudgetPeriod
    included_budgets: List[str] = []  # Budget IDs
    consolidated_total: Decimal
    by_category: Dict[str, Decimal] = {}  # category -> total
    by_department: Dict[str, Decimal] = {}  # department -> total
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class BudgetReport(BaseModel):
    id: str
    report_type: str  # summary, detailed, variance, forecast
    budget_id: Optional[str] = None
    report_name: str
    parameters: Dict[str, Any] = {}
    generated_by: str
    generated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


# ============================================================================
# Storage
# ============================================================================

budgets: Dict[str, Budget] = {}
budget_lines: Dict[str, List[BudgetLine]] = {}
budget_allocations: Dict[str, List[BudgetAllocation]] = {}
budget_revisions: Dict[str, List[BudgetRevision]] = {}
budget_scenarios: Dict[str, List[BudgetScenario]] = {}
budget_forecasts: Dict[str, List[BudgetForecast]] = {}
budget_approvals: Dict[str, List[BudgetApproval]] = {}
variance_analyses: Dict[str, List[VarianceAnalysis]] = {}
consolidations: Dict[str, BudgetConsolidation] = {}


# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    """Health check endpoint"""
    total_budgets = len(budgets)
    approved_budgets = sum(1 for b in budgets.values() if b.status == BudgetStatus.APPROVED)

    return {
        "status": "healthy",
        "service": "budgeting",
        "version": "1.0.0",
        "total_budgets": total_budgets,
        "approved_budgets": approved_budgets,
    }


# --- Budget Management ---

@app.post("/budgets")
async def create_budget(budget: Budget):
    """Create a new budget"""
    budget.id = str(uuid.uuid4())
    budget.created_at = datetime.now(timezone.utc)
    budget.updated_at = datetime.now(timezone.utc)

    budgets[budget.id] = budget
    budget_lines[budget.id] = []
    budget_allocations[budget.id] = []

    return budget


@app.get("/budgets")
async def list_budgets(
    budget_type: Optional[BudgetType] = None,
    status: Optional[BudgetStatus] = None,
    fiscal_year: Optional[str] = None,
    department_id: Optional[str] = None,
    limit: int = 50
):
    """List budgets with filters"""
    results = list(budgets.values())

    if budget_type:
        results = [b for b in results if b.budget_type == budget_type]
    if status:
        results = [b for b in results if b.status == status]
    if fiscal_year:
        results = [b for b in results if b.fiscal_year == fiscal_year]
    if department_id:
        results = [b for b in results if b.department_id == department_id]

    results.sort(key=lambda x: x.created_at, reverse=True)
    return results[:limit]


@app.get("/budgets/{budget_id}")
async def get_budget(budget_id: str):
    """Get budget details"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")
    return budgets[budget_id]


@app.put("/budgets/{budget_id}")
async def update_budget(budget_id: str, budget: Budget):
    """Update budget"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget.id = budget_id
    budget.updated_at = datetime.now(timezone.utc)
    budgets[budget_id] = budget
    return budget


@app.post("/budgets/{budget_id}/submit")
async def submit_budget(budget_id: str, submitted_by: str):
    """Submit budget for approval"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget = budgets[budget_id]
    budget.status = BudgetStatus.SUBMITTED
    budget.updated_at = datetime.now(timezone.utc)

    return {"status": "submitted", "budget_id": budget_id, "submitted_by": submitted_by}


@app.post("/budgets/{budget_id}/approve")
async def approve_budget(budget_id: str, approved_by: str):
    """Approve budget"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget = budgets[budget_id]
    budget.status = BudgetStatus.APPROVED
    budget.approved_by = approved_by
    budget.approved_at = datetime.now(timezone.utc)
    budget.updated_at = datetime.now(timezone.utc)

    return {"status": "approved", "budget_id": budget_id, "approved_by": approved_by}


@app.post("/budgets/{budget_id}/reject")
async def reject_budget(budget_id: str, rejected_by: str, reason: str):
    """Reject budget"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget = budgets[budget_id]
    budget.status = BudgetStatus.REJECTED
    budget.updated_at = datetime.now(timezone.utc)

    return {"status": "rejected", "budget_id": budget_id, "reason": reason}


@app.post("/budgets/{budget_id}/revise")
async def revise_budget(budget_id: str, revision: BudgetRevision):
    """Create budget revision"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    revision.id = str(uuid.uuid4())
    revision.budget_id = budget_id
    revision.revision_date = datetime.now(timezone.utc)

    budget = budgets[budget_id]
    revision.previous_total = budget.total_amount

    if budget_id not in budget_revisions:
        budget_revisions[budget_id] = []
    budget_revisions[budget_id].append(revision)

    # Update budget version
    budget.version += 1
    budget.total_amount = revision.new_total
    budget.status = BudgetStatus.REVISED
    budget.updated_at = datetime.now(timezone.utc)

    return revision


@app.get("/budgets/{budget_id}/revisions")
async def get_budget_revisions(budget_id: str):
    """Get budget revision history"""
    if budget_id not in budget_revisions:
        return []
    return budget_revisions[budget_id]


# --- Budget Lines ---

@app.post("/budgets/{budget_id}/lines")
async def add_budget_line(budget_id: str, line: BudgetLine):
    """Add line item to budget"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    line.id = str(uuid.uuid4())
    line.budget_id = budget_id
    line.remaining_amount = line.annual_amount - line.allocated_amount

    if budget_id not in budget_lines:
        budget_lines[budget_id] = []
    budget_lines[budget_id].append(line)

    # Update budget total
    budget = budgets[budget_id]
    budget.total_amount = sum(l.annual_amount for l in budget_lines[budget_id])
    budget.updated_at = datetime.now(timezone.utc)

    return line


@app.get("/budgets/{budget_id}/lines")
async def get_budget_lines(
    budget_id: str,
    category: Optional[str] = None,
    account_code: Optional[str] = None
):
    """Get budget lines"""
    if budget_id not in budget_lines:
        return []

    lines = budget_lines[budget_id]

    if category:
        lines = [l for l in lines if l.category == category]
    if account_code:
        lines = [l for l in lines if l.account_code == account_code]

    return lines


@app.put("/budgets/{budget_id}/lines/{line_id}")
async def update_budget_line(budget_id: str, line_id: str, line: BudgetLine):
    """Update budget line"""
    if budget_id not in budget_lines:
        raise HTTPException(status_code=404, detail="Budget not found")

    lines = budget_lines[budget_id]
    for i, l in enumerate(lines):
        if l.id == line_id:
            line.id = line_id
            line.budget_id = budget_id
            line.remaining_amount = line.annual_amount - line.allocated_amount
            budget_lines[budget_id][i] = line

            # Update budget total
            budget = budgets[budget_id]
            budget.total_amount = sum(l.annual_amount for l in budget_lines[budget_id])
            return line

    raise HTTPException(status_code=404, detail="Line not found")


# --- Budget Allocations ---

@app.post("/budgets/{budget_id}/allocations")
async def add_budget_allocation(budget_id: str, allocation: BudgetAllocation):
    """Add allocation to budget"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    allocation.id = str(uuid.uuid4())
    allocation.budget_id = budget_id
    allocation.created_at = datetime.now(timezone.utc)

    if budget_id not in budget_allocations:
        budget_allocations[budget_id] = []
    budget_allocations[budget_id].append(allocation)

    return allocation


@app.get("/budgets/{budget_id}/allocations")
async def get_budget_allocations(budget_id: str):
    """Get budget allocations"""
    if budget_id not in budget_allocations:
        return []
    return budget_allocations[budget_id]


# --- Scenarios ---

@app.post("/scenarios")
async def create_scenario(scenario: BudgetScenario):
    """Create budget scenario"""
    scenario.id = str(uuid.uuid4())
    scenario.created_at = datetime.now(timezone.utc)

    if scenario.budget_id not in budget_scenarios:
        budget_scenarios[scenario.budget_id] = []
    budget_scenarios[scenario.budget_id].append(scenario)

    return scenario


@app.get("/budgets/{budget_id}/scenarios")
async def get_budget_scenarios(budget_id: str):
    """Get all scenarios for a budget"""
    if budget_id not in budget_scenarios:
        return []
    return budget_scenarios[budget_id]


@app.post("/scenarios/{scenario_id}/compare")
async def compare_scenarios(scenario_id: str, other_scenario_id: str):
    """Compare two scenarios"""
    # Find scenarios
    scenario = None
    other_scenario = None

    for budget_id, scenarios in budget_scenarios.items():
        for s in scenarios:
            if s.id == scenario_id:
                scenario = s
            if s.id == other_scenario_id:
                other_scenario = s

    if not scenario or not other_scenario:
        raise HTTPException(status_code=404, detail="Scenario not found")

    return {
        "scenario_1": scenario.model_dump(),
        "scenario_2": other_scenario.model_dump(),
        "differences": {
            "growth_rate_diff": scenario.growth_rate - other_scenario.growth_rate,
            "inflation_diff": scenario.inflation_rate - other_scenario.inflation_rate,
            "total_variance": sum(scenario.modified_lines.values()) - sum(other_scenario.modified_lines.values()),
        },
    }


# --- Variance Analysis ---

@app.post("/variance-analysis")
async def run_variance_analysis(
    budget_id: str,
    account_code: str,
    period: str,
    actual_amount: Decimal
):
    """Run variance analysis for a budget line"""
    if budget_id not in budget_lines:
        raise HTTPException(status_code=404, detail="Budget not found")

    # Find the line
    line = None
    for l in budget_lines[budget_id]:
        if l.account_code == account_code:
            line = l
            break

    if not line:
        raise HTTPException(status_code=404, detail="Account not found in budget")

    # Calculate variance
    variance = actual_amount - line.annual_amount
    variance_percentage = float(variance / line.annual_amount * 100) if line.annual_amount else 0

    # Determine variance level
    if abs(variance_percentage) <= 5:
        level = VarianceLevel.GREEN
    elif abs(variance_percentage) <= 15:
        level = VarianceLevel.YELLOW
    else:
        level = VarianceLevel.RED

    analysis = VarianceAnalysis(
        id=str(uuid.uuid4()),
        budget_id=budget_id,
        account_code=account_code,
        account_name=line.account_name,
        period=period,
        budgeted_amount=line.annual_amount,
        actual_amount=actual_amount,
        variance=variance,
        variance_percentage=variance_percentage,
        variance_level=level,
        created_at=datetime.now(timezone.utc),
    )

    if budget_id not in variance_analyses:
        variance_analyses[budget_id] = []
    variance_analyses[budget_id].append(analysis)

    # Update line actual amount
    line.actual_amount = actual_amount
    line.variance = variance
    line.variance_percentage = variance_percentage

    return analysis


@app.get("/budgets/{budget_id}/variance")
async def get_budget_variance(budget_id: str):
    """Get variance analysis for entire budget"""
    if budget_id not in variance_analyses:
        return {
            "budget_id": budget_id,
            "total_variance": "0",
            "analyses": [],
            "summary": {"green": 0, "yellow": 0, "red": 0},
        }

    analyses = variance_analyses[budget_id]
    total_variance = sum(a.variance for a in analyses)

    summary = {
        "green": sum(1 for a in analyses if a.variance_level == VarianceLevel.GREEN),
        "yellow": sum(1 for a in analyses if a.variance_level == VarianceLevel.YELLOW),
        "red": sum(1 for a in analyses if a.variance_level == VarianceLevel.RED),
    }

    return {
        "budget_id": budget_id,
        "total_variance": str(total_variance),
        "analyses": analyses,
        "summary": summary,
    }


# --- Budget Consolidation ---

@app.post("/consolidations")
async def create_consolidation(consolidation: BudgetConsolidation):
    """Create budget consolidation"""
    consolidation.id = str(uuid.uuid4())
    consolidation.created_at = datetime.now(timezone.utc)

    # Calculate consolidated totals
    total = Decimal("0")
    by_category: Dict[str, Decimal] = {}
    by_department: Dict[str, Decimal] = {}

    for budget_id in consolidation.included_budgets:
        if budget_id in budgets:
            budget = budgets[budget_id]
            total += budget.total_amount

            if budget.department_id:
                if budget.department_id not in by_department:
                    by_department[budget.department_id] = Decimal("0")
                by_department[budget.department_id] += budget.total_amount

            if budget_id in budget_lines:
                for line in budget_lines[budget_id]:
                    if line.category not in by_category:
                        by_category[line.category] = Decimal("0")
                    by_category[line.category] += line.annual_amount

    consolidation.consolidated_total = total
    consolidation.by_category = {k: str(v) for k, v in by_category.items()}
    consolidation.by_department = {k: str(v) for k, v in by_department.items()}

    consolidations[consolidation.id] = consolidation
    return consolidation


@app.get("/consolidations")
async def list_consolidations(fiscal_year: Optional[str] = None):
    """List consolidations"""
    results = list(consolidations.values())

    if fiscal_year:
        results = [c for c in results if c.fiscal_year == fiscal_year]

    return results


# --- Reports ---

@app.get("/reports/budget-summary")
async def get_budget_summary(
    budget_id: str,
    include_lines: bool = True,
    include_variance: bool = False
):
    """Generate budget summary report"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget = budgets[budget_id]
    lines = budget_lines.get(budget_id, [])

    by_category = {}
    for line in lines:
        if line.category not in by_category:
            by_category[line.category] = Decimal("0")
        by_category[line.category] += line.annual_amount

    result = {
        "budget": budget.model_dump(),
        "total_amount": str(budget.total_amount),
        "line_count": len(lines),
        "by_category": {k: str(v) for k, v in by_category.items()},
    }

    if include_lines:
        result["lines"] = lines

    if include_variance and budget_id in variance_analyses:
        result["variance"] = variance_analyses[budget_id]

    return result


@app.get("/reports/variance-summary")
async def get_variance_summary(
    budget_id: str,
    period: Optional[str] = None
):
    """Generate variance summary report"""
    if budget_id not in variance_analyses:
        return {"budget_id": budget_id, "variances": [], "summary": {}}

    analyses = variance_analyses[budget_id]

    if period:
        analyses = [a for a in analyses if a.period == period]

    total_budgeted = sum(a.budgeted_amount for a in analyses)
    total_actual = sum(a.actual_amount for a in analyses)
    total_variance = sum(a.variance for a in analyses)

    return {
        "budget_id": budget_id,
        "period": period,
        "total_budgeted": str(total_budgeted),
        "total_actual": str(total_actual),
        "total_variance": str(total_variance),
        "variance_count": len(analyses),
        "by_level": {
            "green": [a for a in analyses if a.variance_level == VarianceLevel.GREEN],
            "yellow": [a for a in analyses if a.variance_level == VarianceLevel.YELLOW],
            "red": [a for a in analyses if a.variance_level == VarianceLevel.RED],
        },
    }


@app.get("/reports/forecast")
async def get_budget_forecast(
    budget_id: str,
    periods: int = 12
):
    """Generate budget forecast"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget = budgets[budget_id]
    lines = budget_lines.get(budget_id, [])

    # Simple forecast based on historical patterns
    forecast_data = []
    monthly_amount = budget.total_amount / 12

    for i in range(periods):
        period_date = budget.start_date + timedelta(days=30 * i)
        forecast_data.append({
            "period": period_date.strftime("%Y-%m"),
            "expected_amount": str(monthly_amount),
            "cumulative": str(monthly_amount * (i + 1)),
        })

    return {
        "budget_id": budget_id,
        "budget_total": str(budget.total_amount),
        "periods": periods,
        "forecast": forecast_data,
    }


@app.get("/reports/comparison")
async def compare_budgets(
    budget_id_1: str,
    budget_id_2: str
):
    """Compare two budgets"""
    if budget_id_1 not in budgets or budget_id_2 not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget_1 = budgets[budget_id_1]
    budget_2 = budgets[budget_id_2]

    lines_1 = budget_lines.get(budget_id_1, [])
    lines_2 = budget_lines.get(budget_id_2, [])

    return {
        "budget_1": {
            "id": budget_1.id,
            "name": budget_1.name,
            "total": str(budget_1.total_amount),
            "lines": len(lines_1),
        },
        "budget_2": {
            "id": budget_2.id,
            "name": budget_2.name,
            "total": str(budget_2.total_amount),
            "lines": len(lines_2),
        },
        "variance": {
            "amount": str(budget_1.total_amount - budget_2.total_amount),
            "percentage": str(
                float((budget_1.total_amount - budget_2.total_amount) / budget_2.total_amount * 100)
                if budget_2.total_amount else 0
            ),
        },
    }


@app.get("/reports/execution")
async def get_budget_execution_report(
    budget_id: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None
):
    """Get budget execution report"""
    if budget_id not in budgets:
        raise HTTPException(status_code=404, detail="Budget not found")

    budget = budgets[budget_id]
    lines = budget_lines.get(budget_id, [])

    execution_data = []
    total_budgeted = Decimal("0")
    total_actual = Decimal("0")

    for line in lines:
        total_budgeted += line.annual_amount
        total_actual += line.actual_amount

        execution_pct = float(line.actual_amount / line.annual_amount * 100) if line.annual_amount else 0

        execution_data.append({
            "account_code": line.account_code,
            "account_name": line.account_name,
            "budgeted": str(line.annual_amount),
            "actual": str(line.actual_amount),
            "variance": str(line.variance),
            "execution_percentage": execution_pct,
            "status": "on_track" if execution_pct <= 110 else "over_budget" if execution_pct > 100 else "under_budget",
        })

    return {
        "budget_id": budget_id,
        "budget_name": budget.name,
        "total_budgeted": str(total_budgeted),
        "total_actual": str(total_actual),
        "total_variance": str(total_budgeted - total_actual),
        "execution_percentage": float(total_actual / total_budgeted * 100) if total_budgeted else 0,
        "line_items": execution_data,
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8099)