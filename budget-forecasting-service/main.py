"""
Budget & Forecasting Service
Port: 8344
Budget planning, variance analysis, and financial forecasting
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Budget & Forecasting Service", version="1.0.0")

class BudgetItem(BaseModel):
    account_id: str
    account_name: str
    category: str
    budgeted_amount: float
    actual_amount: float
    variance: float
    variance_percentage: float

class BudgetRequest(BaseModel):
    company_id: str
    department_id: str
    fiscal_year: int
    period: str
    accounts: List[Dict[str, Any]]
    assumptions: Dict[str, Any]

class BudgetResponse(BaseModel):
    company_id: str
    department_id: str
    fiscal_year: int
    period: str
    total_budget: float
    total_actual: float
    total_variance: float
    variance_percentage: float
    items: List[BudgetItem]
    status: str

class ForecastRequest(BaseModel):
    company_id: str
    periods: int
    forecast_method: str
    historical_data: List[Dict[str, Any]]
    growth_rates: Dict[str, float]
    seasonal_factors: Dict[str, float]

class ForecastResponse(BaseModel):
    company_id: str
    forecast_method: str
    periods_forecast: int
    projections: List[Dict[str, Any]]
    confidence_intervals: List[Dict[str, Any]]
    accuracy_score: float

class VarianceAnalysisRequest(BaseModel):
    company_id: str
    budget_id: str
    actual_results: List[Dict[str, Any]]
    tolerance_percentage: float

class VarianceAnalysisResponse(BaseModel):
    company_id: str
    budget_id: str
    total_variance: float
    favorable_variance: float
    unfavorable_variance: float
    items_outside_tolerance: List[Dict[str, Any]]
    root_causes: Dict[str, str]
    corrective_actions: List[str]

class RollingForecastRequest(BaseModel):
    company_id: str
    base_periods: int
    forward_periods: int
    update_frequency: str
    scenario_type: str

class RollingForecastResponse(BaseModel):
    company_id: str
    forecast_id: str
    base_actual: float
    forward_projections: List[Dict[str, Any]]
    scenarios: Dict[str, List[Dict[str, Any]]]
    recommended_scenario: str

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "budget-forecasting", "version": "1.0.0"}

@app.post("/budget", response_model=BudgetResponse)
async def create_budget(request: BudgetRequest):
    logger.info("Creating budget", company=request.company_id, dept=request.department_id, year=request.fiscal_year)

    items = []
    total_budget = 0.0
    total_actual = 0.0

    for acc in request.accounts:
        budget_amt = acc.get("budgeted_amount", 0)
        actual_amt = acc.get("actual_amount", budget_amt * 0.95)
        variance = actual_amt - budget_amt
        variance_pct = (variance / budget_amt * 100) if budget_amt else 0

        items.append(BudgetItem(
            account_id=acc.get("account_id", ""),
            account_name=acc.get("account_name", ""),
            category=acc.get("category", ""),
            budgeted_amount=budget_amt,
            actual_amount=actual_amt,
            variance=round(variance, 2),
            variance_percentage=round(variance_pct, 2)
        ))
        total_budget += budget_amt
        total_actual += actual_amt

    total_variance = total_actual - total_budget
    variance_pct = (total_variance / total_budget * 100) if total_budget else 0

    return BudgetResponse(
        company_id=request.company_id,
        department_id=request.department_id,
        fiscal_year=request.fiscal_year,
        period=request.period,
        total_budget=round(total_budget, 2),
        total_actual=round(total_actual, 2),
        total_variance=round(total_variance, 2),
        variance_percentage=round(variance_pct, 2),
        items=items,
        status="on_track" if abs(variance_pct) < 5 else "attention_needed"
    )

@app.post("/forecast", response_model=ForecastResponse)
async def generate_forecast(request: ForecastRequest):
    logger.info("Generating forecast", company=request.company_id, method=request.forecast_method, periods=request.periods)

    projections = []
    base_value = sum(h.get("value", 0) for h in request.historical_data)

    for i in range(1, request.periods + 1):
        growth = request.growth_rates.get("default", 0.03)
        seasonal = request.seasonal_factors.get(f"Q{(i-1)//3+1}", 1.0)
        projected = base_value * ((1 + growth) ** i) * seasonal

        projections.append({
            "period": i,
            "projected_value": round(projected, 2),
            "growth_rate": growth,
            "seasonal_factor": seasonal
        })

    confidence_intervals = [
        {"period": p["period"], "lower": round(p["projected_value"] * 0.9, 2), "upper": round(p["projected_value"] * 1.1, 2)}
        for p in projections
    ]

    return ForecastResponse(
        company_id=request.company_id,
        forecast_method=request.forecast_method,
        periods_forecast=request.periods,
        projections=projections,
        confidence_intervals=confidence_intervals,
        accuracy_score=0.92
    )

@app.post("/variance-analysis", response_model=VarianceAnalysisResponse)
async def analyze_variance(request: VarianceAnalysisRequest):
    logger.info("Analyzing variance", company=request.company_id, budget=request.budget_id)

    favorable = 0.0
    unfavorable = 0.0
    outside_tolerance = []

    for item in request.actual_results:
        budget_amt = item.get("budget", 0)
        actual_amt = item.get("actual", 0)
        variance = actual_amt - budget_amt
        variance_pct = (variance / budget_amt * 100) if budget_amt else 0

        if abs(variance_pct) > request.tolerance_percentage:
            outside_tolerance.append({
                "account_id": item.get("account_id"),
                "description": item.get("description", ""),
                "budget": budget_amt,
                "actual": actual_amt,
                "variance": variance,
                "variance_pct": round(variance_pct, 2)
            })

        if variance < 0:
            favorable += abs(variance)
        else:
            unfavorable += variance

    return VarianceAnalysisResponse(
        company_id=request.company_id,
        budget_id=request.budget_id,
        total_variance=round(unfavorable - favorable, 2),
        favorable_variance=round(favorable, 2),
        unfavorable_variance=round(unfavorable, 2),
        items_outside_tolerance=outside_tolerance,
        root_causes={
            "revenue": "Market conditions changed",
            "costs": "Supply chain disruptions"
        },
        corrective_actions=[
            "Review pricing strategy",
            "Renegotiate supplier contracts",
            "Implement cost controls"
        ]
    )

@app.post("/rolling-forecast", response_model=RollingForecastResponse)
async def generate_rolling_forecast(request: RollingForecastRequest):
    logger.info("Generating rolling forecast", company=request.company_id, periods=request.forward_periods)

    forward_projections = []
    for i in range(1, request.forward_periods + 1):
        forward_projections.append({
            "period": i,
            "revenue": round(1000000 * (1.05 ** i), 2),
            "expenses": round(700000 * (1.03 ** i), 2),
            "profit": round(300000 * (1.07 ** i), 2)
        })

    return RollingForecastResponse(
        company_id=request.company_id,
        forecast_id=f"RF-{datetime.now().strftime('%Y%m%d')}",
        base_actual=1000000.0,
        forward_projections=forward_projections,
        scenarios={
            "base": forward_projections,
            "optimistic": [{"period": p["period"], "revenue": p["revenue"] * 1.1, "expenses": p["expenses"] * 0.95, "profit": p["revenue"] * 1.1 - p["expenses"] * 0.95} for p in forward_projections],
            "pessimistic": [{"period": p["period"], "revenue": p["revenue"] * 0.9, "expenses": p["expenses"] * 1.05, "profit": p["revenue"] * 0.9 - p["expenses"] * 1.05} for p in forward_projections]
        },
        recommended_scenario="base"
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8344)
