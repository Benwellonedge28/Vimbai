"""
IFRS 2 Share-Based Payment Service
Port: 8150
Accounts for equity-settled and cash-settled share-based payments
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="IFRS 2 Share-Based Payment Service", version="1.0.0")

# Pydantic Models
class VestingCondition(BaseModel):
    condition_id: str
    condition_type: str  # "service", "performance", "market"
    description: str
    target_value: float
    probability_assessment: float = Field(ge=0, le=1)
    probability_weight: float = Field(ge=0, le=1)

class ShareOption(BaseModel):
    option_id: str
    grant_date: str
    number_of_options: int
    exercise_price: float
    fair_value_per_option: float
    vesting_period_years: int
    expiry_date: str
    vesting_conditions: List[VestingCondition]
    employee_category: str

class ShareBasedPaymentRequest(BaseModel):
    company_id: str
    reporting_date: str
    equity_settled_grants: List[ShareOption]
    cash_settled_grants: List[ShareOption]
    shares_issued: int
    share_price_at_reporting: float

class VestingCalculation(BaseModel):
    option_id: str
    grant_date: str
    options_granted: int
    options_expected_to_vest: int
    forfeiture_rate: float
    cumulative_expense: float
    period_expense: float

class IFRS2Response(BaseModel):
    company_id: str
    reporting_date: str
    equity_settled_expense: float
    cash_settled_expense: float
    total_expense: float
    equity_settled_vesting: List[VestingCalculation]
    cash_settled_vesting: List[VestingCalculation]
    liability_for_cash_settled: float
    equity_reserve: float
    dilutive_effect: int

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal Vimbai service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "ifrs-2-share-based-payment", "version": "1.0.0"}

@app.post("/calculate", response_model=IFRS2Response)
async def calculate_share_based_payment(request: ShareBasedPaymentRequest):
    """Calculate share-based payment expenses and reserves."""
    logger.info("Processing IFRS 2", company=request.company_id)

    equity_vesting = []
    cash_vesting = []
    total_equity_expense = 0.0
    total_cash_expense = 0.0

    # Equity-settled grants
    for option in request.equity_settled_grants:
        # Calculate expected vesting based on conditions
        expected_vesting = option.number_of_options
        forfeiture_rate = 0.05  # Base forfeiture

        for condition in option.vesting_conditions:
            if condition.condition_type != "market":
                expected_vesting *= condition.probability_assessment
                forfeiture_rate += (1 - condition.probability_weight)

        options_to_expense = int(expected_vesting * (1 - min(0.3, forfeiture_rate)))

        # Cumulative expense over vesting period
        per_year_expense = options_to_expense * option.fair_value_per_option / option.vesting_period_years
        cumulative_expense = per_year_expense * 2  # Assume 2 years elapsed

        equity_vesting.append(VestingCalculation(
            option_id=option.option_id,
            grant_date=option.grant_date,
            options_granted=option.number_of_options,
            options_expected_to_vest=options_to_expense,
            forfeiture_rate=forfeiture_rate,
            cumulative_expense=cumulative_expense,
            period_expense=per_year_expense
        ))
        total_equity_expense += per_year_expense

    # Cash-settled grants
    for option in request.cash_settled_grants:
        expected_vesting = option.number_of_options
        for condition in option.vesting_conditions:
            if condition.condition_type != "market":
                expected_vesting *= condition.probability_assessment

        options_to_expense = int(expected_vesting)

        # Cash-settled: liability = (current share price - exercise price) × options
        current_intrinsic = max(0, request.share_price_at_reporting - option.exercise_price)
        liability = current_intrinsic * options_to_expense * 0.5  # Assume 50% vested

        per_year_expense = options_to_expense * option.fair_value_per_option / option.vesting_period_years

        cash_vesting.append(VestingCalculation(
            option_id=option.option_id,
            grant_date=option.grant_date,
            options_granted=option.number_of_options,
            options_expected_to_vest=options_to_expense,
            forfeiture_rate=0.0,
            cumulative_expense=liability,
            period_expense=per_year_expense
        ))
        total_cash_expense += per_year_expense

    # Dilutive effect for EPS
    total_options = sum(o.number_of_options for o in request.equity_settled_grants + request.cash_settled_grants)
    avg_price = request.share_price_at_reporting
    exercise_price = sum(o.exercise_price * o.number_of_options for o in request.equity_settled_grants) / total_options if total_options > 0 else 0
    dilutive_effect = total_options * (1 - exercise_price / avg_price) if avg_price > exercise_price else 0

    response = IFRS2Response(
        company_id=request.company_id,
        reporting_date=request.reporting_date,
        equity_settled_expense=total_equity_expense,
        cash_settled_expense=total_cash_expense,
        total_expense=total_equity_expense + total_cash_expense,
        equity_settled_vesting=equity_vesting,
        cash_settled_vesting=cash_vesting,
        liability_for_cash_settled=sum(v.cumulative_expense for v in cash_vesting),
        equity_reserve=sum(v.cumulative_expense for v in equity_vesting),
        dilutive_effect=int(dilutive_effect)
    )

    logger.info("IFRS 2 calculation complete", total_expense=response.total_expense)
    return response

@app.post("/fair-value-equity-instruments")
async def calculate_fair_value_equity(
    option_type: str,  # "plain_vanilla", "barrier", "lookback", "ESOP"
    spot_price: float,
    exercise_price: float,
    time_to_expiry: float,
    risk_free_rate: float,
    volatility: float,
    dividend_yield: float
):
    """Calculate fair value of equity instruments using Black-Scholes variants."""
    import math

    # Simplified Black-Scholes for plain vanilla
    d1 = (math.log(spot_price / exercise_price) + (risk_free_rate + volatility ** 2 / 2) * time_to_expiry) / (volatility * math.sqrt(time_to_expiry))
    d2 = d1 - volatility * math.sqrt(time_to_expiry)

    call_value = spot_price * math.exp(-dividend_yield * time_to_expiry) * 0.5 * (1 + math.exp(d1) + math.exp(-d1))
    put_value = exercise_price * math.exp(-risk_free_rate * time_to_expiry) * 0.5

    # Adjust based on option type
    if option_type == "lookback":
        # Lookback option has higher value
        adjustment_factor = 1.2
    elif option_type == "barrier":
        adjustment_factor = 0.8
    else:
        adjustment_factor = 1.0

    fair_value = call_value * adjustment_factor

    return {
        "option_type": option_type,
        "spot_price": spot_price,
        "exercise_price": exercise_price,
        "time_to_expiry": time_to_expiry,
        "risk_free_rate": risk_free_rate,
        "volatility": volatility,
        "dividend_yield": dividend_yield,
        "d1": d1,
        "d2": d2,
        "calculated_fair_value": fair_value,
        "adjustment_applied": adjustment_factor
    }

@app.post("/performance-condition")
async def assess_performance_condition(
    condition_type: str,
    target_metric: float,
    current_metric: float,
    measurement_period: float,
    probability_based_on_regression: float
):
    """Assess probability of performance condition achievement."""
    achievement_rate = current_metric / target_metric if target_metric > 0 else 0
    time_remaining_pct = 1 - (measurement_period / 3)  # Assume 3-year vesting

    if condition_type == "EPS_growth":
        expected_achievement = min(1.0, achievement_rate * (1 + time_remaining_pct * 0.1))
    elif condition_type == "TSR":
        expected_achievement = probability_based_on_regression
    else:
        expected_achievement = min(1.0, achievement_rate)

    return {
        "condition_type": condition_type,
        "target_metric": target_metric,
        "current_metric": current_metric,
        "achievement_rate": achievement_rate,
        "measurement_period": measurement_period,
        "expected_achievement_probability": expected_achievement,
        "expense_multiplier": expected_achievement
    }

@app.post("/modification")
async def account_for_modification(
    original_grant_date: str,
    original_fair_value: float,
    original_options: int,
    modification_date: str,
    new_fair_value: float,
    new_options: int,
    new_vesting_period: int,
    years_elapsed: int,
    cumulative_expense_original: float
):
    """Account for modification of share-based payment arrangement."""
    # Calculate incremental fair value
    incremental_value = new_fair_value - original_fair_value
    if incremental_value < 0:
        incremental_value = 0  # Don't reduce expense

    # Remaining vesting period
    remaining_period = max(1, new_vesting_period - years_elapsed)

    # New total fair value to expense
    total_value = original_fair_value + incremental_value
    per_period_new = total_value / new_vesting_period
    per_period_incremental = incremental_value / remaining_period

    # Catch-up expense for modification
    original_cumulative = cumulative_expense_original
    modified_cumulative = per_period_new * years_elapsed
    catch_up_expense = max(0, modified_cumulative - original_cumulative)

    return {
        "original_grant_date": original_grant_date,
        "modification_date": modification_date,
        "original_fair_value": original_fair_value,
        "new_fair_value": new_fair_value,
        "incremental_fair_value": incremental_value,
        "original_options": original_options,
        "new_options": new_options,
        "years_elapsed": years_elapsed,
        "remaining_vesting_period": remaining_period,
        "catch_up_expense": catch_up_expense,
        "prospective_expense": per_period_incremental,
        "total_modification_impact": catch_up_expense + per_period_incremental
    }

@app.post("/group-transaction")
async def account_for_group_transaction(
    parent_equity_settled: float,
    parent_cash_settled: float,
    non_controlling_interests: float,
    transaction_type: str  # "equity_for_equity", "cash_for_equity"
):
    """Account for share-based payment in group transactions."""
    if transaction_type == "equity_for_equity":
        treatment = "equity-settled - no change in accounting"
        ncI_impact = 0.0
    else:  # cash_for_equity
        treatment = "cash-settled - remeasurement required"
        ncI_impact = non_controlling_interests * 0.1  # 10% allocated to NCI

    return {
        "transaction_type": transaction_type,
        "parent_equity_settled": parent_equity_settled,
        "parent_cash_settled": parent_cash_settled,
        "non_controlling_interests": non_controlling_interests,
        "ncI_impact": ncI_impact,
        "treatment": treatment,
        "financial_statement_impact": "equity reserve" if transaction_type == "equity_for_equity" else "liability"
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8150)
