from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse
from typing import List, Optional, Dict, Any
from neo4j import AsyncSession
from finance_service import models, crud
from finance_service.database import init_db_schema, Neo4jConnector
from finance_service.dependencies import get_db_session, get_jwt_token, get_user_id # Added get_user_id
from finance_service.utils.auth import check_permission
from finance_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv
from datetime import datetime
from pydantic import ValidationError as PydanticValidationError # NEW: to catch pydantic's internal validation errors
from decimal import Decimal # Added Decimal for potential future use or consistency


# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Finance Service",
    description="Manages budgets, tracks financial performance, and generates financial ratios.",
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
    Neo4jConnector.close_driver()

# --- Global Exception Handlers ---
@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request, exc: NotFoundError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(ConflictError)
async def conflict_exception_handler(request, exc: ConflictError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(ValidationError)
async def validation_exception_handler(request, exc: ValidationError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )
    
@app.exception_handler(UnauthorizedError)
async def unauthorized_exception_handler(request, exc: UnauthorizedError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
        headers={"WWW-Authenticate": "Bearer"},
    )

@app.exception_handler(ForbiddenError)
async def forbidden_exception_handler(request, exc: ForbiddenError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(PydanticValidationError) # NEW: Catch Pydantic's internal validation errors
async def pydantic_validation_exception_handler(request, exc: PydanticValidationError):
    errors = exc.errors()
    error_details = []
    for error in errors:
        loc = ".".join(map(str, error["loc"]))
        error_details.append(f"Field '{loc}': {error['msg']}")
    return JSONResponse(
        status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
        content={"detail": "Validation error: " + "; ".join(error_details), "code": "PYDANTIC_VALIDATION_ERROR"},
    )

# --- Budget Endpoints ---
@app.post("/budgets/", response_model=models.BudgetInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.budgets"))])
async def create_new_budget(budget: models.BudgetCreate, db_session: AsyncSession = Depends(get_db_session)):
    # Check for overlapping budget periods
    existing_budgets = await crud.get_all_budgets(db_session)
    for eb in existing_budgets:
        if not (budget.end_date <= eb.start_date or budget.start_date >= eb.end_date):
            raise ConflictError(detail="Budget period overlaps with an existing budget.", code="BUDGET_PERIOD_OVERLAP")
    return await crud.create_budget(db_session, budget)

@app.get("/budgets/", response_model=List[models.BudgetInDB],
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def read_all_budgets(db_session: AsyncSession = Depends(get_db_session)):
    return await crud.get_all_budgets(db_session)

@app.get("/budgets/{budget_id}", response_model=models.BudgetInDB,
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def read_budget_by_id(budget_id: str, db_session: AsyncSession = Depends(get_db_session)):
    db_budget = await crud.get_budget(db_session, budget_id)
    if db_budget is None:
        raise NotFoundError(detail="Budget not found.")
    return db_budget
    
@app.put("/budgets/{budget_id}", response_model=models.BudgetInDB,
             dependencies=[Depends(check_permission("finance.write.budgets"))])
async def update_existing_budget(
    budget_id: str,
    budget: models.BudgetUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    db_budget = await crud.update_budget(db_session, budget_id, budget)
    if db_budget is None:
        raise NotFoundError(detail="Budget not found.")
    return db_budget

@app.delete("/budgets/{budget_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.budgets"))])
async def delete_existing_budget(budget_id: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_budget(db_session, budget_id)
    if not success:
        raise NotFoundError(detail="Budget not found.")
    return {"ok": True}

# --- Budget Item Endpoints ---
@app.post("/budgets/{budget_id}/items/", response_model=models.BudgetItemInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("finance.write.budget_items"))])
async def create_new_budget_item(
    budget_id: str,
    item: models.BudgetItemCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    db_budget = await crud.get_budget(db_session, budget_id)
    if db_budget is None:
        raise NotFoundError(detail="Parent budget not found.", code="BUDGET_NOT_FOUND")
    
    # Optionally, validate account_number against Accounting Service's COA
    # This would involve an httpx call to API_GATEWAY_URL/accounts/{account_number}
    
    return await crud.create_budget_item(db_session, budget_id, item)

@app.get("/budgets/{budget_id}/items/{item_id}", response_model=models.BudgetItemInDB,
             dependencies=[Depends(check_permission("finance.read.budget_items"))])
async def read_budget_item_by_id(
    budget_id: str,
    item_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    db_item = await crud.get_budget_item(db_session, budget_id, item_id)
    if db_item is None:
        raise NotFoundError(detail="Budget item not found.", code="BUDGET_ITEM_NOT_FOUND")
    return db_item

@app.put("/budgets/{budget_id}/items/{item_id}", response_model=models.BudgetItemInDB,
             dependencies=[Depends(check_permission("finance.write.budget_items"))])
async def update_existing_budget_item(
    budget_id: str,
    item_id: str,
    item: models.BudgetItemUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    db_item = await crud.update_budget_item(db_session, budget_id, item_id, item)
    if db_item is None:
        raise NotFoundError(detail="Budget item not found.", code="BUDGET_ITEM_NOT_FOUND")
    return db_item

@app.delete("/budgets/{budget_id}/items/{item_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("finance.delete.budget_items"))])
async def delete_existing_budget_item(
    budget_id: str,
    item_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_budget_item(db_session, budget_id, item_id)
    if not success:
        raise NotFoundError(detail="Budget item not found.", code="BUDGET_ITEM_NOT_FOUND")
    return {"ok": True}

# --- NEW: Budget Reporting Endpoints ---
@app.get("/budgets/{budget_id}/variance-report", response_model=models.BudgetVarianceReport,
             dependencies=[Depends(check_permission("finance.read.budgets"))])
async def get_budget_variance_report_endpoint(
    budget_id: str,
    user_id: str = Depends(get_user_id), # Assuming get_user_id is available in finance_service.dependencies
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_budget_variance_report(db_session, user_id, budget_id, jwt_token)


# --- Financial Analysis Endpoints ---
# ... (unchanged) ...

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Finance Service is running!"}
