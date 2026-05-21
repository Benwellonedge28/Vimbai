from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.responses import JSONResponse
from typing import List, Optional, Dict, Any
from neo4j import AsyncSession
from finance_service import models, crud
from finance_service.database import init_db_schema, Neo4jConnector
from finance_service.dependencies import get_db_session, get_user_id
from finance_service.utils.auth import check_permission
from finance_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
from finance_service.services.scenario_engine import ScenarioEngine # NEW
from datetime import datetime, timedelta
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Finance Service",
    description="Provides financial planning, budgeting, forecasting, and scenario analysis capabilities.",
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
    await init_db_schema()

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
    budget.user_id = user_id
    return await crud.create_budget(db_session, budget)

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

@app.get("/budgets/", response_model=List[models.BudgetInDB],
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def get_all_budgets(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_budgets(db_session, user_id)

@app.put("/budgets/{budget_id}", response_model=models.BudgetInDB,
             dependencies=[Depends(check_permission("finance.write.budgets"))])
async def update_budget(
    budget_id: str,
    budget_update: models.BudgetUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_budget = await crud.update_budget(db_session, budget_id, budget_update)
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


# --- Forecast Endpoints ---
@app.post("/forecasts/baseline", response_model=models.ForecastInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.forecasts"))])
async def generate_baseline_forecast(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: datetime = datetime.now(),
    end_date: datetime = datetime.now() + timedelta(days=365),
    interval: str = "monthly" # monthly, quarterly, yearly
):
    scenario_engine = ScenarioEngine(db_session)
    forecast = await scenario_engine.generate_baseline_forecast(user_id, start_date, end_date, interval)
    return forecast

@app.post("/forecasts/{baseline_forecast_id}/scenario", response_model=models.ForecastInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.forecasts"))])
async def apply_scenario_to_forecast(
    baseline_forecast_id: str,
    scenario_params: models.ScenarioParametersCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    scenario_engine = ScenarioEngine(db_session)
    try:
        scenario_forecast = await scenario_engine.apply_scenario(baseline_forecast_id, scenario_params)
        return scenario_forecast
    except NotFoundError as e:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=str(e))
    except ValueError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))

@app.get("/forecasts/{forecast_id}", response_model=models.ForecastInDB,
             dependencies=[Depends(check_permission("finance.read.forecasts"))])
async def get_forecast_by_id(
    forecast_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    forecast = await crud.get_forecast(db_session, forecast_id)
    if forecast is None:
        raise NotFoundError(detail="Forecast not found.")
    return forecast

@app.get("/forecasts/", response_model=List[models.ForecastInDB],
             dependencies=[Depends(check_permission("finance.read.forecasts"))])
async def get_all_forecasts(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_forecasts(db_session, user_id)

@app.post("/forecasts/compare", response_model=Dict[str, Any],
              dependencies=[Depends(check_permission("finance.read.forecasts"))])
async def compare_forecast_scenarios(
    forecast_ids: List[str],
    db_session: AsyncSession = Depends(get_db_session)
):
    scenario_engine = ScenarioEngine(db_session)
    comparison_results = await scenario_engine.compare_scenarios(forecast_ids)
    return comparison_results

@app.put("/forecasts/{forecast_id}", response_model=models.ForecastInDB,
             dependencies=[Depends(check_permission("finance.write.forecasts"))])
async def update_forecast(
    forecast_id: str,
    forecast_update: models.ForecastUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_forecast = await crud.update_forecast(db_session, forecast_id, forecast_update)
    if updated_forecast is None:
        raise NotFoundError(detail="Forecast not found.")
    return updated_forecast

@app.delete("/forecasts/{forecast_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.forecasts"))])
async def delete_forecast(
    forecast_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_forecast(db_session, forecast_id)
    if not success:
        raise NotFoundError(detail="Forecast not found.")
    return {"ok": True}


# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Finance Service is running!"}
