from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from finance_service import models, crud
from finance_service.database import init_db_schema, Neo4jConnector
from finance_service.dependencies import get_db_session, get_user_id
from finance_service.utils.auth import check_permission
from finance_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Finance Service",
    description="Manages budgeting, financial forecasting, and scenario analysis.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j")
    )
    Neo4jConnector.get_driver()
    await init_db_schema() # Initialize Neo4j schema specific to finance service

@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()

# --- Global Exception Handlers ---
@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)

@app.exception_handler(ConflictError)
async def conflict_exception_handler(request, exc: ConflictError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)

@app.exception_handler(ValidationError)
async def validation_exception_handler(request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)
    
@app.exception_handler(UnauthorizedError)
async def unauthorized_exception_handler(request, exc: UnauthorizedError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail, headers={"WWW-Authenticate": "Bearer"})

@app.exception_handler(ForbiddenError)
async def forbidden_exception_handler(request, exc: ForbiddenError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)


# --- Budget Endpoints ---
@app.post("/budgets/", response_model=models.BudgetInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.budgets"))])
async def create_budget(
    budget: models.BudgetCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_budget(db_session, budget, user_id)

@app.get("/budgets/", response_model=List[models.BudgetInDB],
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def get_budgets_by_user(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_budgets_by_user(db_session, user_id)

@app.get("/budgets/{budget_id}", response_model=models.BudgetInDB,
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def get_budget_by_id(
    budget_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    budget = await crud.get_budget(db_session, budget_id)
    if budget is None:
        raise NotFoundError(detail="Budget not found.")
    return budget

@app.put("/budgets/{budget_id}", response_model=models.BudgetInDB,
             dependencies=[Depends(check_permission("finance.write.budgets"))])
async def update_budget(
    budget_id: str,
    budget: models.BudgetUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_budget = await crud.update_budget(db_session, budget_id, budget)
    if updated_budget is None:
        raise NotFoundError(detail="Budget not found.")
    return updated_budget

@app.delete("/budgets/{budget_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.budgets"))])
async def delete_budget(
    budget_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_budget(db_session, budget_id)
    if not success:
        raise NotFoundError(detail="Budget not found.")
    return {"ok": True}

# --- Budget Item Endpoints ---
@app.post("/budgets/{budget_id}/items/", response_model=models.BudgetItemInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.budget_items"))])
async def create_budget_item(
    budget_id: str,
    item: models.BudgetItemCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_budget_item(db_session, budget_id, item)

@app.put("/budget-items/{item_id}", response_model=models.BudgetItemInDB,
             dependencies=[Depends(check_permission("finance.write.budget_items"))])
async def update_budget_item(
    item_id: str,
    item: models.BudgetItemUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_item = await crud.update_budget_item(db_session, item_id, item)
    if updated_item is None:
        raise NotFoundError(detail="Budget Item not found.")
    return updated_item

@app.delete("/budget-items/{item_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.budget_items"))])
async def delete_budget_item(
    item_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_budget_item(db_session, item_id)
    if not success:
        raise NotFoundError(detail="Budget Item not found.")
    return {"ok": True}

# --- Variance Analysis Endpoints ---
@app.get("/budgets/{budget_id}/variance-report", response_model=models.BudgetVarianceReport,
             dependencies=[Depends(check_permission("finance.read.reports"))])
async def get_budget_variance_report_endpoint(
    budget_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    # This is a placeholder. Actual implementation would involve querying actuals from accounting service
    # and comparing with budget items.
    raise HTTPException(status_code=status.HTTP_501_NOT_IMPLEMENTED, detail="Budget Variance Report generation not yet implemented.")

# --- Financial Ratios Endpoints ---
@app.get("/financial-ratios", response_model=models.FinancialRatiosReport,
             dependencies=[Depends(check_permission("finance.read.reports"))])
async def get_financial_ratios_endpoint(
    start_date: str, # Should be date type
    end_date: str,   # Should be date type
    db_session: AsyncSession = Depends(get_db_session)
):
    # This is a placeholder. Actual implementation would involve querying data from accounting service
    # and calculating ratios.
    raise HTTPException(status_code=status.HTTP_501_NOT_IMPLEMENTED, detail="Financial Ratios Report generation not yet implemented.")


# --- Financial Forecast Endpoints (NEW) ---
@app.post("/forecasts/", response_model=models.FinancialForecastInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.forecasts"))])
async def create_financial_forecast(
    forecast: models.FinancialForecastCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    forecast.owner_user_id = user_id
    return await crud.create_financial_forecast(db_session, forecast)

@app.get("/forecasts/", response_model=List[models.FinancialForecastInDB],
             dependencies=[Depends(check_permission("finance.read.forecasts"))])
async def get_all_financial_forecasts(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_financial_forecasts_by_user(db_session, user_id)

@app.get("/forecasts/{forecast_id}", response_model=models.FinancialForecastInDB,
             dependencies=[Depends(check_permission("finance.read.forecasts"))])
async def get_financial_forecast_by_id(
    forecast_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    forecast = await crud.get_financial_forecast(db_session, forecast_id)
    if forecast is None:
        raise NotFoundError(detail="Financial Forecast not found.")
    return forecast

@app.put("/forecasts/{forecast_id}", response_model=models.FinancialForecastInDB,
             dependencies=[Depends(check_permission("finance.write.forecasts"))])
async def update_financial_forecast(
    forecast_id: str,
    forecast: models.FinancialForecastUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_forecast = await crud.update_financial_forecast(db_session, forecast_id, forecast)
    if updated_forecast is None:
        raise NotFoundError(detail="Financial Forecast not found.")
    return updated_forecast

@app.delete("/forecasts/{forecast_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.forecasts"))])
async def delete_financial_forecast(
    forecast_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_financial_forecast(db_session, forecast_id)
    if not success:
        raise NotFoundError(detail="Financial Forecast not found.")
    return {"ok": True}

# --- Scenario Endpoints (NEW) ---
@app.post("/scenarios/", response_model=models.ScenarioInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.scenarios"))])
async def create_scenario(
    scenario: models.ScenarioCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    scenario.owner_user_id = user_id
    return await crud.create_scenario(db_session, scenario)

@app.get("/scenarios/", response_model=List[models.ScenarioInDB],
             dependencies=[Depends(check_permission("finance.read.scenarios"))])
async def get_all_scenarios(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_scenarios_by_user(db_session, user_id)

@app.get("/scenarios/{scenario_id}", response_model=models.ScenarioInDB,
             dependencies=[Depends(check_permission("finance.read.scenarios"))])
async def get_scenario_by_id(
    scenario_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    scenario = await crud.get_scenario(db_session, scenario_id)
    if scenario is None:
        raise NotFoundError(detail="Scenario not found.")
    return scenario

@app.put("/scenarios/{scenario_id}", response_model=models.ScenarioInDB,
             dependencies=[Depends(check_permission("finance.write.scenarios"))])
async def update_scenario(
    scenario_id: str,
    scenario: models.ScenarioUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_scenario = await crud.update_scenario(db_session, scenario_id, scenario)
    if updated_scenario is None:
        raise NotFoundError(detail="Scenario not found.")
    return updated_scenario

@app.delete("/scenarios/{scenario_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.scenarios"))])
async def delete_scenario(
    scenario_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_scenario(db_session, scenario_id)
    if not success:
        raise NotFoundError(detail="Scenario not found.")
    return {"ok": True}

@app.post("/scenarios/{scenario_id}/analyze", response_model=models.ScenarioAnalysisResult,
              dependencies=[Depends(check_permission("finance.execute.scenario_analysis"))])
async def analyze_scenario(
    scenario_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    # This is a placeholder for actual scenario analysis logic.
    # It would involve fetching the scenario, its base forecast, applying parameters,
    # and generating a new projected forecast.
    scenario = await crud.get_scenario(db_session, scenario_id)
    if not scenario:
        raise NotFoundError(detail="Scenario not found.")

    base_forecast = None
    if scenario.base_forecast_id:
        base_forecast = await crud.get_financial_forecast(db_session, scenario.base_forecast_id)

    # Placeholder for actual analysis:
    # In a real implementation, this would involve a complex calculation engine
    # applying scenario parameters to the base forecast or historical data.
    # For now, it returns a mock result.

    projected_data_points = []
    if base_forecast and base_forecast.data_points:
        for dp in base_forecast.data_points:
            # Simple example: apply a global revenue growth factor if defined in scenario
            adjusted_amount = dp.amount
            for param in scenario.parameters:
                if param.target_metric == dp.value_type and param.param_type == "percentage":
                    adjusted_amount = adjusted_amount * Decimal(str(1 + param.value))
            projected_data_points.append(models.FinancialForecastDataPoint(
                period=dp.period,
                value_type=dp.value_type,
                amount=adjusted_amount
            ))

    return models.ScenarioAnalysisResult(
        scenario_id=scenario_id,
        forecast_id=scenario.base_forecast_id,
        analysis_date=datetime.utcnow(),
        summary=f"Analysis of scenario '{scenario.name}' completed. Parameters applied.",
        projected_metrics={"total_revenue_scenario": sum(dp.amount for dp in projected_data_points if dp.value_type == "revenue")},
        projected_data_points=projected_data_points,
    )


# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Finance Service is running!"}
