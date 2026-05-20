from fastapi import FastAPI, Depends, HTTPException, status
from typing import List, Optional
from neo4j import AsyncSession
from finance_service import models, crud
from finance_service.database import init_db_schema, Neo4jConnector
from finance_service.dependencies import get_db_session
from finance_service.utils.auth import check_permission
import os
from dotenv import load_dotenv

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

@app.post("/budgets/", response_model=models.BudgetInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.budgets"))])
async def create_new_budget(budget: models.BudgetCreate, db_session: AsyncSession = Depends(get_db_session)):
    # Optionally, check if a budget with the same fiscal_year and period already exists
    # For simplicity, we allow multiple budgets for now.
    return await crud.create_budget(db_session, budget)

@app.get("/budgets/{budget_id}", response_model=models.BudgetInDB,
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def read_budget_by_id(budget_id: str, db_session: AsyncSession = Depends(get_db_session)):
    db_budget = await crud.get_budget(db_session, budget_id)
    if db_budget is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Budget not found")
    return db_budget
        
@app.get("/budgets/", response_model=List[models.BudgetInDB],
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def read_all_budgets(db_session: AsyncSession = Depends(get_db_session)):
    return await crud.get_all_budgets(db_session)

@app.put("/budgets/{budget_id}", response_model=models.BudgetInDB,
             dependencies=[Depends(check_permission("finance.write.budgets"))])
async def update_existing_budget(budget_id: str, budget: models.BudgetUpdate, db_session: AsyncSession = Depends(get_db_session)):
    db_budget = await crud.update_budget(db_session, budget_id, budget)
    if db_budget is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Budget not found")
    return db_budget

@app.delete("/budgets/{budget_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.budgets"))])
async def delete_existing_budget(budget_id: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_budget(db_session, budget_id)
    if not success:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Budget not found")
    return {"ok": True}

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Finance Service is running!"}
