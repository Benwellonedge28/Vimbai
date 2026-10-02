"""Pydantic models for the Scenario Modeling Service (unchanged contracts)."""

import uuid
from datetime import date, datetime
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field, validator


class ScenarioType(str, Enum):
    """Types of scenario modeling"""

    BUDGET_FORECAST = "budget_forecast"
    REVENUE_PROJECTION = "revenue_projection"
    EXPENSE_SIMULATION = "expense_simulation"
    CASH_FLOW = "cash_flow"
    PROFITABILITY = "profitability"
    GROWTH_RATE = "growth_rate"
    CUSTOM = "custom"


class RuleConditionOperator(str, Enum):
    """Operators for rule conditions"""

    EQUALS = "equals"
    NOT_EQUALS = "not_equals"
    GREATER_THAN = "greater_than"
    LESS_THAN = "less_than"
    GREATER_OR_EQUAL = "greater_or_equal"
    LESS_OR_EQUAL = "less_or_equal"
    CONTAINS = "contains"
    BETWEEN = "between"


class RuleActionType(str, Enum):
    """Types of rule actions"""

    ADJUST_AMOUNT = "adjust_amount"
    SCALE_AMOUNT = "scale_amount"
    APPLY_PERCENTAGE = "apply_percentage"
    SET_VALUE = "set_value"
    FLAG_ALERT = "flag_alert"
    TRIGGER_WORKFLOW = "trigger_workflow"


class RuleCondition(BaseModel):
    """Condition for rule evaluation"""

    field: str = Field(..., description="Field to evaluate")
    operator: RuleConditionOperator = Field(..., description="Comparison operator")
    value: Any = Field(..., description="Value to compare against")
    secondary_value: Optional[Any] = Field(None, description="Secondary value for BETWEEN operator")


class RuleAction(BaseModel):
    """Action to execute when rule condition is met"""

    action_type: RuleActionType = Field(..., description="Type of action")
    target_field: str = Field(..., description="Field to modify")
    value: Any = Field(..., description="Action value or multiplier")
    description: Optional[str] = Field(None, description="Action description")


class ModelingRule(BaseModel):
    """Rule for scenario modeling"""

    id: str = Field(default_factory=lambda: str(uuid.uuid4()), description="Rule ID")
    name: str = Field(..., max_length=200, description="Rule name")
    description: Optional[str] = Field(None, description="Rule description")
    conditions: List[RuleCondition] = Field(..., min_length=1, description="Rule conditions (AND logic)")
    actions: List[RuleAction] = Field(..., min_length=1, description="Actions to execute")
    priority: int = Field(100, description="Rule priority (lower = higher priority)")
    enabled: bool = Field(True, description="Whether rule is enabled")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)

    @validator("conditions", "actions")
    def validate_not_empty(cls, v):
        if len(v) == 0:
            raise ValueError("Conditions and actions must have at least one item")
        return v


class ScenarioVariable(BaseModel):
    """Variable for What-If analysis"""

    name: str = Field(..., max_length=100, description="Variable name")
    current_value: Decimal = Field(..., description="Current/base value")
    min_value: Optional[Decimal] = Field(None, description="Minimum allowed value")
    max_value: Optional[Decimal] = Field(None, description="Maximum allowed value")
    step: Optional[Decimal] = Field(None, description="Step size for sensitivity analysis")
    unit: Optional[str] = Field(None, max_length=50, description="Unit of measurement")
    description: Optional[str] = Field(None, description="Variable description")


class ScenarioCreate(BaseModel):
    """Create a new scenario"""

    name: str = Field(..., max_length=200, description="Scenario name")
    description: Optional[str] = Field(None, max_length=1000, description="Scenario description")
    scenario_type: ScenarioType = Field(..., description="Type of scenario")
    base_date: date = Field(..., description="Base date for calculations")
    end_date: date = Field(..., description="End date for projection")
    variables: List[ScenarioVariable] = Field(default_factory=list, description="Scenario variables")
    rules: List[str] = Field(default_factory=list, description="Rule IDs to apply")
    assumptions: Optional[Dict[str, Any]] = Field(None, description="Additional assumptions")


class ScenarioInDB(ScenarioCreate):
    """Scenario as stored in database"""

    id: str = Field(..., description="Scenario ID")
    user_id: str = Field(..., description="User who created the scenario")
    status: Literal["draft", "active", "completed", "archived"] = Field("draft", description="Scenario status")
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    results: Optional[Dict[str, Any]] = Field(None, description="Scenario results")

    class Config:
        from_attributes = True


class WhatIfAnalysisCreate(BaseModel):
    """What-If analysis request"""

    scenario_id: str = Field(..., description="Scenario to use")
    variable_changes: Dict[str, Decimal] = Field(..., description="Variable changes to apply")
    description: Optional[str] = Field(None, description="Analysis description")


class WhatIfResult(BaseModel):
    """What-If analysis result"""

    analysis_id: str = Field(..., description="Analysis ID")
    scenario_id: str = Field(..., description="Scenario used")
    base_values: Dict[str, Decimal] = Field(..., description="Base variable values")
    changed_values: Dict[str, Decimal] = Field(..., description="Changed variable values")
    original_outcome: Decimal = Field(..., description="Original outcome")
    new_outcome: Decimal = Field(..., description="New outcome after changes")
    variance: Decimal = Field(..., description="Variance from original")
    variance_percent: float = Field(..., description="Variance as percentage")
    affected_accounts: List[Dict[str, Any]] = Field(default_factory=list, description="Accounts affected")
    timestamp: datetime = Field(default_factory=datetime.utcnow)


class SensitivityAnalysisRequest(BaseModel):
    """Sensitivity analysis request"""

    scenario_id: str = Field(..., description="Scenario to use")
    variable_name: str = Field(..., description="Variable to analyze")
    min_change: Decimal = Field(..., description="Minimum change to apply")
    max_change: Decimal = Field(..., description="Maximum change to apply")
    steps: int = Field(10, ge=2, le=100, description="Number of steps")


class SensitivityResult(BaseModel):
    """Sensitivity analysis result"""

    analysis_id: str = Field(..., description="Analysis ID")
    variable_name: str = Field(..., description="Variable analyzed")
    outcomes: List[Dict[str, Any]] = Field(..., description="Outcome for each step")
    most_sensitive_range: Dict[str, Any] = Field(..., description="Range of highest sensitivity")
    recommendations: List[str] = Field(..., description="Analysis recommendations")
