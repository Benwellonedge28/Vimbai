from fastapi import FastAPI, Depends, HTTPException, status, Query
from typing import List, Optional, Dict, Any
from neo4j import AsyncSession
from finance_service import models, crud
from finance_service.database import init_db_schema, Neo4jConnector
from finance_service.dependencies import get_db_session
from finance_service.utils.auth import check_permission
from finance_service.dependencies import get_jwt_token # NEW: to get JWT for internal calls
import os
from dotenv import load_dotenv
from datetime import datetime

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Finance Service",
    description="Manages Budgets, Financial Analysis, Forecasting, and Valuation.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.get_driver() # Initialize driver
    await init_db_schema() # Ensure schema and constraints

@app.on_event("shutdown")
async def shutdown_event():
    Neo4jConnector.close_driver() # Close driver

# --- Budget Endpoints ---
# ... (existing budget endpoints) ...

# --- Financial Analysis Endpoints ---

@app.get("/budgets/{budget_id}/variance-report", response_model=models.BudgetVarianceReport,
             dependencies=[Depends(check_permission("finance.read.variance_reports"))])
async def get_budget_variance_report(budget_id: str, db_session: AsyncSession = Depends(get_db_session)):
    report = await crud.generate_budget_variance_report(db_session, budget_id)
    if report is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Budget not found")
    return report
    
@app.get("/financial-ratios", response_model=models.FinancialRatiosReport,
             dependencies=[Depends(check_permission("finance.read.financial_ratios"))])
async def get_financial_ratios(
    start_date: datetime = Query(..., description="Start date of the reporting period (ISO format)"),
    end_date: datetime = Query(..., description="End date of the reporting period (ISO format)"),
    jwt_token: str = Depends(get_jwt_token) # Get JWT for internal calls
):
    if start_date >= end_date:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Start date must be before end date")
    
    return await crud.generate_financial_ratios_report(jwt_token, start_date, end_date)


# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Finance Service is running!"}
