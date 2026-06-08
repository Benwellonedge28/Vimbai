"""
Scenario Modeling Service - Rule-based What-If Analysis
======================================================

This service implements rule-based scenario modeling for financial forecasting
and What-If analysis as specified in the FinAcc Design Document.

Features:
- Create and manage financial scenarios
- Rule-based forecasting with condition-action pairs
- What-If analysis with variable manipulation
- Sensitivity analysis
- Scenario comparison
- Trend extrapolation
"""

from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse
from typing import List, Optional, Dict, Any
from pydantic import BaseModel, Field, validator
from datetime import datetime, date
from decimal import Decimal
from enum import Enum
import uuid
import json
import os
from dotenv import load_dotenv

load_dotenv()

# =============================================================================
# MODELS
# =============================================================================

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

    @validator('conditions', 'actions')
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

# =============================================================================
# IN-MEMORY STORAGE (Production would use Neo4j)
# =============================================================================

class Storage:
    scenarios: Dict[str, ScenarioInDB] = {}
    rules: Dict[str, ModelingRule] = {}
    what_if_results: Dict[str, WhatIfResult] = {}
    sensitivity_results: Dict[str, SensitivityResult] = {}

storage = Storage()

# =============================================================================
# FASTAPI APPLICATION
# =============================================================================

app = FastAPI(
    title="FinAcc Scenario Modeling Service",
    description="Rule-based What-If analysis and financial forecasting",
    version="1.0.0"
)

# =============================================================================
# HELPER FUNCTIONS
# =============================================================================

def evaluate_condition(condition: RuleCondition, data: Dict[str, Any]) -> bool:
    """Evaluate a single rule condition"""
    value = data.get(condition.field)

    if value is None:
        return False

    try:
        value = Decimal(str(value))
    except (ValueError, TypeError):
        pass

    target_value = condition.value
    try:
        target_value = Decimal(str(target_value))
    except (ValueError, TypeError):
        pass

    if condition.operator == RuleConditionOperator.EQUALS:
        return value == target_value
    elif condition.operator == RuleConditionOperator.NOT_EQUALS:
        return value != target_value
    elif condition.operator == RuleConditionOperator.GREATER_THAN:
        return value > target_value
    elif condition.operator == RuleConditionOperator.LESS_THAN:
        return value < target_value
    elif condition.operator == RuleConditionOperator.GREATER_OR_EQUAL:
        return value >= target_value
    elif condition.operator == RuleConditionOperator.LESS_OR_EQUAL:
        return value <= target_value
    elif condition.operator == RuleConditionOperator.CONTAINS:
        return str(target_value) in str(value)
    elif condition.operator == RuleConditionOperator.BETWEEN:
        secondary = Decimal(str(condition.secondary_value))
        return target_value <= value <= secondary

    return False

def apply_rule(rule: ModelingRule, data: Dict[str, Any]) -> Dict[str, Any]:
    """Apply a rule to data and return modified data"""
    # Check all conditions (AND logic)
    conditions_met = all(evaluate_condition(c, data) for c in rule.conditions)

    if not conditions_met:
        return data

    # Apply actions
    result = data.copy()
    for action in rule.actions:
        if action.action_type == RuleActionType.ADJUST_AMOUNT:
            current = Decimal(str(result.get(action.target_field, 0)))
            result[action.target_field] = float(current + Decimal(str(action.value)))
        elif action.action_type == RuleActionType.SCALE_AMOUNT:
            current = Decimal(str(result.get(action.target_field, 0)))
            result[action.target_field] = float(current * Decimal(str(action.value)))
        elif action.action_type == RuleActionType.APPLY_PERCENTAGE:
            current = Decimal(str(result.get(action.target_field, 0)))
            percentage = Decimal(str(action.value)) / Decimal('100')
            result[action.target_field] = float(current * (Decimal('1') + percentage))
        elif action.action_type == RuleActionType.SET_VALUE:
            result[action.target_field] = action.value

    return result

def calculate_outcome(scenario: ScenarioInDB, variables: Dict[str, Decimal]) -> Decimal:
    """Calculate the outcome based on scenario type and variables"""
    # Simple calculation based on scenario type
    if scenario.scenario_type == ScenarioType.BUDGET_FORECAST:
        total = sum(variables.values())
        return total
    elif scenario.scenario_type == ScenarioType.REVENUE_PROJECTION:
        base = Decimal(str(scenario.assumptions.get("base_revenue", 100000)) if scenario.assumptions else 100000)
        growth = variables.get("growth_rate", Decimal('0.05'))
        periods = (scenario.end_date - scenario.base_date).days / 30
        return base * (Decimal('1') + growth) ** Decimal(str(periods))
    elif scenario.scenario_type == ScenarioType.EXPENSE_SIMULATION:
        return sum(v for k, v in variables.items() if "expense" in k.lower())
    elif scenario.scenario_type == ScenarioType.CASH_FLOW:
        inflows = sum(v for k, v in variables.items() if "inflow" in k.lower())
        outflows = sum(v for k, v in variables.items() if "outflow" in k.lower())
        return inflows - outflows
    elif scenario.scenario_type == ScenarioType.PROFITABILITY:
        revenue = variables.get("revenue", Decimal('0'))
        costs = variables.get("costs", Decimal('0'))
        return revenue - costs
    else:
        return sum(variables.values())

# =============================================================================
# SCENARIO ENDPOINTS
# =============================================================================

@app.post("/scenarios/", response_model=ScenarioInDB, status_code=status.HTTP_201_CREATED)
async def create_scenario(
    scenario: ScenarioCreate,
    user_id: str = Query(..., description="User ID")
):
    """Create a new scenario"""
    scenario_id = str(uuid.uuid4())

    # Validate rules exist
    for rule_id in scenario.rules:
        if rule_id not in storage.rules:
            raise HTTPException(status_code=404, detail=f"Rule {rule_id} not found")

    scenario_data = ScenarioInDB(
        id=scenario_id,
        user_id=user_id,
        name=scenario.name,
        description=scenario.description,
        scenario_type=scenario.scenario_type,
        base_date=scenario.base_date,
        end_date=scenario.end_date,
        variables=scenario.variables,
        rules=scenario.rules,
        assumptions=scenario.assumptions
    )

    storage.scenarios[scenario_id] = scenario_data
    return scenario_data

@app.get("/scenarios/", response_model=List[ScenarioInDB])
async def list_scenarios(
    user_id: str = Query(..., description="User ID"),
    status: Optional[str] = Query(None, description="Filter by status"),
    scenario_type: Optional[ScenarioType] = Query(None, description="Filter by type")
):
    """List all scenarios for a user"""
    results = [
        s for s in storage.scenarios.values()
        if s.user_id == user_id
    ]

    if status:
        results = [s for s in results if s.status == status]
    if scenario_type:
        results = [s for s in results if s.scenario_type == scenario_type]

    return sorted(results, key=lambda x: x.created_at, reverse=True)

@app.get("/scenarios/{scenario_id}", response_model=ScenarioInDB)
async def get_scenario(scenario_id: str, user_id: str = Query(...)):
    """Get a scenario by ID"""
    if scenario_id not in storage.scenarios:
        raise HTTPException(status_code=404, detail="Scenario not found")

    scenario = storage.scenarios[scenario_id]
    if scenario.user_id != user_id:
        raise HTTPException(status_code=403, detail="Access denied")

    return scenario

@app.put("/scenarios/{scenario_id}", response_model=ScenarioInDB)
async def update_scenario(
    scenario_id: str,
    updates: Dict[str, Any],
    user_id: str = Query(...)
):
    """Update a scenario"""
    if scenario_id not in storage.scenarios:
        raise HTTPException(status_code=404, detail="Scenario not found")

    scenario = storage.scenarios[scenario_id]
    if scenario.user_id != user_id:
        raise HTTPException(status_code=403, detail="Access denied")

    # Apply updates
    for key, value in updates.items():
        if hasattr(scenario, key) and key not in ['id', 'user_id', 'created_at']:
            setattr(scenario, key, value)

    scenario.updated_at = datetime.utcnow()
    return scenario

@app.delete("/scenarios/{scenario_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_scenario(scenario_id: str, user_id: str = Query(...)):
    """Delete a scenario"""
    if scenario_id not in storage.scenarios:
        raise HTTPException(status_code=404, detail="Scenario not found")

    scenario = storage.scenarios[scenario_id]
    if scenario.user_id != user_id:
        raise HTTPException(status_code=403, detail="Access denied")

    del storage.scenarios[scenario_id]

# =============================================================================
# MODELING RULES ENDPOINTS
# =============================================================================

@app.post("/rules/", response_model=ModelingRule, status_code=status.HTTP_201_CREATED)
async def create_rule(rule: ModelingRule):
    """Create a new modeling rule"""
    storage.rules[rule.id] = rule
    return rule

@app.get("/rules/", response_model=List[ModelingRule])
async def list_rules(enabled_only: bool = Query(False, description="Filter enabled only")):
    """List all modeling rules"""
    if enabled_only:
        return [r for r in storage.rules.values() if r.enabled]
    return list(storage.rules.values())

@app.get("/rules/{rule_id}", response_model=ModelingRule)
async def get_rule(rule_id: str):
    """Get a rule by ID"""
    if rule_id not in storage.rules:
        raise HTTPException(status_code=404, detail="Rule not found")
    return storage.rules[rule_id]

@app.put("/rules/{rule_id}", response_model=ModelingRule)
async def update_rule(rule_id: str, updates: Dict[str, Any]):
    """Update a rule"""
    if rule_id not in storage.rules:
        raise HTTPException(status_code=404, detail="Rule not found")

    rule = storage.rules[rule_id]
    for key, value in updates.items():
        if hasattr(rule, key) and key not in ['id', 'created_at']:
            setattr(rule, key, value)
    rule.updated_at = datetime.utcnow()

    return rule

@app.delete("/rules/{rule_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_rule(rule_id: str):
    """Delete a rule"""
    if rule_id not in storage.rules:
        raise HTTPException(status_code=404, detail="Rule not found")
    del storage.rules[rule_id]

# =============================================================================
# WHAT-IF ANALYSIS ENDPOINTS
# =============================================================================

@app.post("/what-if/", response_model=WhatIfResult, status_code=status.HTTP_201_CREATED)
async def run_what_if_analysis(analysis: WhatIfAnalysisCreate):
    """Run a What-If analysis"""
    if analysis.scenario_id not in storage.scenarios:
        raise HTTPException(status_code=404, detail="Scenario not found")

    scenario = storage.scenarios[analysis.scenario_id]

    # Get base values
    base_values = {v.name: v.current_value for v in scenario.variables}

    # Apply changes
    changed_values = base_values.copy()
    for var_name, new_value in analysis.variable_changes.items():
        if var_name in changed_values:
            changed_values[var_name] = new_value

    # Calculate outcomes
    original_outcome = calculate_outcome(scenario, base_values)
    new_outcome = calculate_outcome(scenario, changed_values)

    variance = new_outcome - original_outcome
    variance_percent = float(variance / original_outcome * 100) if original_outcome != 0 else 0

    # Find affected accounts (simplified)
    affected_accounts = [
        {"account": "Revenue", "variance": float(variance * Decimal('0.4'))},
        {"account": "Expenses", "variance": float(variance * Decimal('0.3'))},
        {"account": "Net Income", "variance": float(variance * Decimal('0.3'))}
    ]

    result = WhatIfResult(
        analysis_id=str(uuid.uuid4()),
        scenario_id=analysis.scenario_id,
        base_values=base_values,
        changed_values=changed_values,
        original_outcome=original_outcome,
        new_outcome=new_outcome,
        variance=variance,
        variance_percent=variance_percent,
        affected_accounts=affected_accounts
    )

    storage.what_if_results[result.analysis_id] = result
    return result

@app.get("/what-if/{analysis_id}", response_model=WhatIfResult)
async def get_what_if_result(analysis_id: str):
    """Get What-If analysis result"""
    if analysis_id not in storage.what_if_results:
        raise HTTPException(status_code=404, detail="Analysis not found")
    return storage.what_if_results[analysis_id]

@app.get("/what-if/scenario/{scenario_id}", response_model=List[WhatIfResult])
async def get_scenario_what_if_results(scenario_id: str):
    """Get all What-If analyses for a scenario"""
    return [
        r for r in storage.what_if_results.values()
        if r.scenario_id == scenario_id
    ]

# =============================================================================
# SENSITIVITY ANALYSIS ENDPOINTS
# =============================================================================

@app.post("/sensitivity/", response_model=SensitivityResult, status_code=status.HTTP_201_CREATED)
async def run_sensitivity_analysis(request: SensitivityAnalysisRequest):
    """Run sensitivity analysis on a variable"""
    if request.scenario_id not in storage.scenarios:
        raise HTTPException(status_code=404, detail="Scenario not found")

    scenario = storage.scenarios[request.scenario_id]

    # Find the variable
    variable = next((v for v in scenario.variables if v.name == request.variable_name), None)
    if not variable:
        raise HTTPException(status_code=404, detail=f"Variable {request.variable_name} not found")

    # Calculate outcomes for each step
    outcomes = []
    base_values = {v.name: v.current_value for v in scenario.variables}

    step_size = (request.max_change - request.min_change) / Decimal(str(request.steps - 1))

    for i in range(request.steps):
        change_value = request.min_change + (step_size * Decimal(str(i)))
        test_values = base_values.copy()
        test_values[request.variable_name] = variable.current_value + change_value

        outcome = calculate_outcome(scenario, test_values)
        outcomes.append({
            "change": float(change_value),
            "outcome": float(outcome),
            "change_percent": float(change_value / variable.current_value * 100) if variable.current_value != 0 else 0
        })

    # Find most sensitive range (largest outcome change per unit)
    max_sensitivity = 0
    most_sensitive_range = {}

    for i in range(len(outcomes) - 1):
        delta_change = outcomes[i + 1]["change"] - outcomes[i]["change"]
        delta_outcome = outcomes[i + 1]["outcome"] - outcomes[i]["outcome"]
        sensitivity = abs(delta_outcome / delta_change) if delta_change != 0 else 0

        if sensitivity > max_sensitivity:
            max_sensitivity = sensitivity
            most_sensitive_range = {
                "min_change": outcomes[i]["change"],
                "max_change": outcomes[i + 1]["change"],
                "outcome_impact": float(abs(delta_outcome))
            }

    # Generate recommendations
    recommendations = []
    for o in outcomes:
        if o["outcome"] > outcomes[0]["outcome"] * 1.1:
            recommendations.append(f"Increase {request.variable_name} by {o['change_percent']:.1f}% yields {o['outcome']:.2f}")
        elif o["outcome"] < outcomes[0]["outcome"] * 0.9:
            recommendations.append(f"Decrease {request.variable_name} by {abs(o['change_percent']):.1f}% yields {o['outcome']:.2f}")

    result = SensitivityResult(
        analysis_id=str(uuid.uuid4()),
        variable_name=request.variable_name,
        outcomes=outcomes,
        most_sensitive_range=most_sensitive_range,
        recommendations=recommendations[:5]  # Top 5 recommendations
    )

    storage.sensitivity_results[result.analysis_id] = result
    return result

# =============================================================================
# SCENARIO COMPARISON
# =============================================================================

@app.post("/compare/", response_model=Dict[str, Any])
async def compare_scenarios(scenario_ids: List[str]):
    """Compare multiple scenarios"""
    scenarios = []
    for sid in scenario_ids:
        if sid not in storage.scenarios:
            raise HTTPException(status_code=404, detail=f"Scenario {sid} not found")
        scenarios.append(storage.scenarios[sid])

    comparison = {
        "scenarios": [
            {
                "id": s.id,
                "name": s.name,
                "type": s.scenario_type.value,
                "base_date": s.base_date.isoformat(),
                "end_date": s.end_date.isoformat(),
                "variables": {v.name: float(v.current_value) for v in s.variables}
            }
            for s in scenarios
        ],
        "comparison_date": datetime.utcnow().isoformat(),
        "best_case": None,
        "worst_case": None,
        "recommendations": []
    }

    # Calculate outcomes for each scenario
    outcomes = {}
    for s in scenarios:
        variables = {v.name: v.current_value for v in s.variables}
        outcome = calculate_outcome(s, variables)
        outcomes[s.id] = float(outcome)

    comparison["outcomes"] = outcomes

    if outcomes:
        best_id = max(outcomes, key=outcomes.get)
        worst_id = min(outcomes, key=outcomes.get)

        comparison["best_case"] = {
            "scenario_id": best_id,
            "outcome": outcomes[best_id],
            "name": next(s.name for s in scenarios if s.id == best_id)
        }
        comparison["worst_case"] = {
            "scenario_id": worst_id,
            "outcome": outcomes[worst_id],
            "name": next(s.name for s in scenarios if s.id == worst_id)
        }

    return comparison

# =============================================================================
# HEALTH CHECK
# =============================================================================

@app.get("/health")
async def health_check():
    """Service health check"""
    return {
        "status": "healthy",
        "service": "scenario-modeling",
        "version": "1.0.0",
        "scenarios_count": len(storage.scenarios),
        "rules_count": len(storage.rules)
    }

# =============================================================================
# MAIN
# =============================================================================

if __name__ == "__main__":
    import uvicorn
    port = int(os.getenv("SCENARIO_MODELING_PORT", "8092"))
    uvicorn.run(app, host="0.0.0.0", port=port)