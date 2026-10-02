"""Vimbai Scenario Modeling Service - rule-based What-If analysis and forecasting.

This file may be imported bare (uvicorn main:app), so it bootstraps its
own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "scenario_modeling_service" not in _sys.modules or not hasattr(
    _sys.modules.get("scenario_modeling_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("scenario_modeling_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["scenario_modeling_service"] = _pkg
    _sys.modules["scenario_modeling_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List

from fastapi import Depends, FastAPI, HTTPException, Query, Request, status
from neo4j import AsyncSession
from scenario_modeling_service import crud, models
from scenario_modeling_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "scenario-modeling-service"
PORT = int(os.getenv("PORT", "8122"))

app = FastAPI(
    title="Vimbai Scenario Modeling Service",
    description="Rule-based What-If analysis and financial forecasting",
    version="2.0.0",
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


# =============================================================================
# HELPER FUNCTIONS (pure computation, unchanged)
# =============================================================================


def evaluate_condition(condition: models.RuleCondition, data: Dict[str, Any]) -> bool:
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

    if condition.operator == models.RuleConditionOperator.EQUALS:
        return value == target_value
    elif condition.operator == models.RuleConditionOperator.NOT_EQUALS:
        return value != target_value
    elif condition.operator == models.RuleConditionOperator.GREATER_THAN:
        return value > target_value
    elif condition.operator == models.RuleConditionOperator.LESS_THAN:
        return value < target_value
    elif condition.operator == models.RuleConditionOperator.GREATER_OR_EQUAL:
        return value >= target_value
    elif condition.operator == models.RuleConditionOperator.LESS_OR_EQUAL:
        return value <= target_value
    elif condition.operator == models.RuleConditionOperator.CONTAINS:
        return str(target_value) in str(value)
    elif condition.operator == models.RuleConditionOperator.BETWEEN:
        secondary = Decimal(str(condition.secondary_value))
        return target_value <= value <= secondary

    return False


def apply_rule(rule: models.ModelingRule, data: Dict[str, Any]) -> Dict[str, Any]:
    """Apply a rule to data and return modified data"""
    # Check all conditions (AND logic)
    conditions_met = all(evaluate_condition(c, data) for c in rule.conditions)

    if not conditions_met:
        return data

    # Apply actions
    result = data.copy()
    for action in rule.actions:
        if action.action_type == models.RuleActionType.ADJUST_AMOUNT:
            current = Decimal(str(result.get(action.target_field, 0)))
            result[action.target_field] = float(current + Decimal(str(action.value)))
        elif action.action_type == models.RuleActionType.SCALE_AMOUNT:
            current = Decimal(str(result.get(action.target_field, 0)))
            result[action.target_field] = float(current * Decimal(str(action.value)))
        elif action.action_type == models.RuleActionType.APPLY_PERCENTAGE:
            current = Decimal(str(result.get(action.target_field, 0)))
            percentage = Decimal(str(action.value)) / Decimal("100")
            result[action.target_field] = float(current * (Decimal("1") + percentage))
        elif action.action_type == models.RuleActionType.SET_VALUE:
            result[action.target_field] = action.value

    return result


def calculate_outcome(scenario: models.ScenarioInDB, variables: Dict[str, Decimal]) -> Decimal:
    """Calculate the outcome based on scenario type and variables"""
    # Simple calculation based on scenario type
    if scenario.scenario_type == models.ScenarioType.BUDGET_FORECAST:
        total = sum(variables.values())
        return total
    elif scenario.scenario_type == models.ScenarioType.REVENUE_PROJECTION:
        base = Decimal(str(scenario.assumptions.get("base_revenue", 100000)) if scenario.assumptions else 100000)
        growth = variables.get("growth_rate", Decimal("0.05"))
        periods = (scenario.end_date - scenario.base_date).days / 30
        return base * (Decimal("1") + growth) ** Decimal(str(periods))
    elif scenario.scenario_type == models.ScenarioType.EXPENSE_SIMULATION:
        return sum(v for k, v in variables.items() if "expense" in k.lower())
    elif scenario.scenario_type == models.ScenarioType.CASH_FLOW:
        inflows = sum(v for k, v in variables.items() if "inflow" in k.lower())
        outflows = sum(v for k, v in variables.items() if "outflow" in k.lower())
        return inflows - outflows
    elif scenario.scenario_type == models.ScenarioType.PROFITABILITY:
        revenue = variables.get("revenue", Decimal("0"))
        costs = variables.get("costs", Decimal("0"))
        return revenue - costs
    else:
        return sum(variables.values())


# =============================================================================
# SCENARIO ENDPOINTS
# =============================================================================


@app.post("/scenarios/", response_model=models.ScenarioInDB, status_code=status.HTTP_201_CREATED)
async def create_scenario(
    scenario: models.ScenarioCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new scenario"""
    scenario_id = str(uuid.uuid4())

    # Validate rules exist among the caller's Book-visible rules
    for rule_id in scenario.rules:
        rule = await crud.find_rule(db_session, user_id, rule_id)
        if rule is None:
            raise HTTPException(status_code=404, detail=f"Rule {rule_id} not found")

    scenario_data = models.ScenarioInDB(
        id=scenario_id,
        user_id=user_id,
        name=scenario.name,
        description=scenario.description,
        scenario_type=scenario.scenario_type,
        base_date=scenario.base_date,
        end_date=scenario.end_date,
        variables=scenario.variables,
        rules=scenario.rules,
        assumptions=scenario.assumptions,
    )

    await crud.create_scenario(db_session, user_id, scenario_data)
    return scenario_data


@app.get("/scenarios/", response_model=List[models.ScenarioInDB])
async def list_scenarios(
    status: str = Query(None, description="Filter by status"),
    scenario_type: models.ScenarioType = Query(None, description="Filter by type"),
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all scenarios for a user (Book-visible)"""
    return await crud.list_scenarios(
        db_session,
        user_id,
        status=status or "",
        scenario_type=scenario_type.value if scenario_type else "",
    )


@app.get("/scenarios/{scenario_id}", response_model=models.ScenarioInDB)
async def get_scenario(
    scenario_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a scenario by ID"""
    scenario = await crud.find_scenario(db_session, user_id, scenario_id)
    if scenario is None:
        raise HTTPException(status_code=404, detail="Scenario not found")
    return scenario


@app.put("/scenarios/{scenario_id}", response_model=models.ScenarioInDB)
async def update_scenario(
    scenario_id: str,
    updates: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a scenario"""
    scenario = await crud.find_scenario(db_session, user_id, scenario_id)
    if scenario is None:
        raise HTTPException(status_code=404, detail="Scenario not found")

    # Apply updates (original setattr semantics)
    for key, value in updates.items():
        if hasattr(scenario, key) and key not in ["id", "user_id", "created_at"]:
            setattr(scenario, key, value)

    scenario.updated_at = datetime.now(timezone.utc)
    await crud.update_scenario(db_session, user_id, scenario)
    return scenario


@app.delete("/scenarios/{scenario_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_scenario(
    scenario_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a scenario"""
    scenario = await crud.find_scenario(db_session, user_id, scenario_id)
    if scenario is None:
        raise HTTPException(status_code=404, detail="Scenario not found")
    await crud.delete_scenario(db_session, user_id, scenario_id)


# =============================================================================
# MODELING RULES ENDPOINTS
# =============================================================================


@app.post("/rules/", response_model=models.ModelingRule, status_code=status.HTTP_201_CREATED)
async def create_rule(
    rule: models.ModelingRule,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new modeling rule"""
    await crud.create_rule(db_session, user_id, rule)
    return rule


@app.get("/rules/", response_model=List[models.ModelingRule])
async def list_rules(
    enabled_only: bool = Query(False, description="Filter enabled only"),
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all modeling rules for the caller (Book-visible)"""
    return await crud.list_rules(db_session, user_id, enabled_only=enabled_only)


@app.get("/rules/{rule_id}", response_model=models.ModelingRule)
async def get_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a rule by ID"""
    rule = await crud.find_rule(db_session, user_id, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Rule not found")
    return rule


@app.put("/rules/{rule_id}", response_model=models.ModelingRule)
async def update_rule(
    rule_id: str,
    updates: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a rule"""
    rule = await crud.find_rule(db_session, user_id, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Rule not found")

    # Apply updates (original setattr semantics)
    for key, value in updates.items():
        if hasattr(rule, key) and key not in ["id", "created_at"]:
            setattr(rule, key, value)
    rule.updated_at = datetime.now(timezone.utc)

    await crud.update_rule(db_session, user_id, rule)
    return rule


@app.delete("/rules/{rule_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Delete a rule"""
    rule = await crud.find_rule(db_session, user_id, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Rule not found")
    await crud.delete_rule(db_session, user_id, rule_id)


# =============================================================================
# WHAT-IF ANALYSIS ENDPOINTS
# =============================================================================


@app.post("/what-if/", response_model=models.WhatIfResult, status_code=status.HTTP_201_CREATED)
async def run_what_if_analysis(
    analysis: models.WhatIfAnalysisCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Run a What-If analysis on a caller-owned scenario"""
    scenario = await crud.find_scenario(db_session, user_id, analysis.scenario_id)
    if scenario is None:
        raise HTTPException(status_code=404, detail="Scenario not found")

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
        {"account": "Revenue", "variance": float(variance * Decimal("0.4"))},
        {"account": "Expenses", "variance": float(variance * Decimal("0.3"))},
        {"account": "Net Income", "variance": float(variance * Decimal("0.3"))},
    ]

    result = models.WhatIfResult(
        analysis_id=str(uuid.uuid4()),
        scenario_id=analysis.scenario_id,
        base_values=base_values,
        changed_values=changed_values,
        original_outcome=original_outcome,
        new_outcome=new_outcome,
        variance=variance,
        variance_percent=variance_percent,
        affected_accounts=affected_accounts,
    )

    await crud.create_what_if(db_session, user_id, result)
    return result


@app.get("/what-if/{analysis_id}", response_model=models.WhatIfResult)
async def get_what_if_result(
    analysis_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get What-If analysis result"""
    result = await crud.get_what_if(db_session, user_id, analysis_id)
    if result is None:
        raise HTTPException(status_code=404, detail="Analysis not found")
    return result


@app.get("/what-if/scenario/{scenario_id}", response_model=List[models.WhatIfResult])
async def get_scenario_what_if_results(
    scenario_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get all What-If analyses for a scenario"""
    return await crud.list_what_if_for_scenario(db_session, user_id, scenario_id)


# =============================================================================
# SENSITIVITY ANALYSIS ENDPOINTS
# =============================================================================


@app.post("/sensitivity/", response_model=models.SensitivityResult, status_code=status.HTTP_201_CREATED)
async def run_sensitivity_analysis(
    request: models.SensitivityAnalysisRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Run sensitivity analysis on a variable of a caller-owned scenario"""
    scenario = await crud.find_scenario(db_session, user_id, request.scenario_id)
    if scenario is None:
        raise HTTPException(status_code=404, detail="Scenario not found")

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
        outcomes.append(
            {
                "change": float(change_value),
                "outcome": float(outcome),
                "change_percent": (
                    float(change_value / variable.current_value * 100) if variable.current_value != 0 else 0
                ),
            }
        )

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
                "outcome_impact": float(abs(delta_outcome)),
            }

    # Generate recommendations
    recommendations = []
    for o in outcomes:
        if o["outcome"] > outcomes[0]["outcome"] * 1.1:
            recommendations.append(
                f"Increase {request.variable_name} by {o['change_percent']:.1f}% yields {o['outcome']:.2f}"
            )
        elif o["outcome"] < outcomes[0]["outcome"] * 0.9:
            recommendations.append(
                f"Decrease {request.variable_name} by {abs(o['change_percent']):.1f}% yields {o['outcome']:.2f}"
            )

    result = models.SensitivityResult(
        analysis_id=str(uuid.uuid4()),
        variable_name=request.variable_name,
        outcomes=outcomes,
        most_sensitive_range=most_sensitive_range,
        recommendations=recommendations[:5],  # Top 5 recommendations
    )

    await crud.create_sensitivity(db_session, user_id, request.scenario_id, result)
    return result


# =============================================================================
# SCENARIO COMPARISON
# =============================================================================


@app.post("/compare/", response_model=Dict[str, Any])
async def compare_scenarios(
    scenario_ids: List[str],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Compare multiple scenarios (caller-owned, Book-visible)"""
    scenarios = []
    for sid in scenario_ids:
        scenario = await crud.find_scenario(db_session, user_id, sid)
        if scenario is None:
            raise HTTPException(status_code=404, detail=f"Scenario {sid} not found")
        scenarios.append(scenario)

    comparison = {
        "scenarios": [
            {
                "id": s.id,
                "name": s.name,
                "type": s.scenario_type.value,
                "base_date": s.base_date.isoformat(),
                "end_date": s.end_date.isoformat(),
                "variables": {v.name: float(v.current_value) for v in s.variables},
            }
            for s in scenarios
        ],
        "comparison_date": datetime.now(timezone.utc).isoformat(),
        "best_case": None,
        "worst_case": None,
        "recommendations": [],
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
            "name": next(s.name for s in scenarios if s.id == best_id),
        }
        comparison["worst_case"] = {
            "scenario_id": worst_id,
            "outcome": outcomes[worst_id],
            "name": next(s.name for s in scenarios if s.id == worst_id),
        }

    return comparison


# =============================================================================
# MAIN
# =============================================================================

if __name__ == "__main__":
    import uvicorn

    port = int(os.getenv("SCENARIO_MODELING_PORT", PORT))
    uvicorn.run(app, host="0.0.0.0", port=port)
