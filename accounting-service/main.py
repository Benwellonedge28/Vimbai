from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from accounting_service import models, crud
from accounting_service.database import init_db_schema, Neo4jConnector
from accounting_service.dependencies import get_db_session, get_user_id, get_jwt_token
from accounting_service.utils.auth import check_permission
from accounting_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv
from datetime import datetime
from pydantic import ValidationError as PydanticValidationError

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Accounting Service",
    description="Manages Chart of Accounts, Journal Entries, Ledgers, Trial Balance, and Financial Statements.",
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

@app.exception_handler(PydanticValidationError)
async def pydantic_validation_exception_handler(request, exc: PydanticValidationError):
    errors = exc.errors()
    error_details = []
    for error in errors:
        loc = ".".join(map(str, error["loc"]))
        error_details.append(f"Field '{loc}': {error["msg"]}")
    return JSONResponse(
        status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
        content={"detail": "Validation error: " + "; ".join(error_details), "code": "PYDANTIC_VALIDATION_ERROR"},
    )

# --- Chart of Accounts Endpoints ---
# ... (unchanged) ...

# --- Journal Entry Endpoints ---
@app.post("/journal-entries/", response_model=models.JournalEntryInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("accounting.write.journal_entries"))])
async def create_new_journal_entry(
    journal_entry: models.JournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token), # Pass JWT for internal calls
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_journal_entry(db_session, user_id, journal_entry, jwt_token)

@app.get("/journal-entries/{entry_id}", response_model=models.JournalEntryInDB,
             dependencies=[Depends(check_permission("accounting.read.journal_entries"))])
async def read_journal_entry_by_id(
    entry_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_entry = await crud.get_journal_entry(db_session, user_id, entry_id)
    if db_entry is None:
        raise NotFoundError(detail="Journal entry not found.")
    return db_entry

@app.get("/journal-entries/", response_model=List[models.JournalEntryInDB],
             dependencies=[Depends(check_permission("accounting.read.journal_entries"))])
async def read_all_journal_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None, description="Start date for filtering journal entries (ISO format)."),
    end_date: Optional[datetime] = Query(None, description="End date for filtering journal entries (ISO format).")
):
    return await crud.get_all_journal_entries(db_session, user_id, start_date, end_date)

@app.put("/journal-entries/{entry_id}", response_model=models.JournalEntryInDB,
             dependencies=[Depends(check_permission("accounting.write.journal_entries"))])
async def update_existing_journal_entry(
    entry_id: str,
    journal_entry: models.JournalEntryUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_entry = await crud.update_journal_entry(db_session, user_id, entry_id, journal_entry)
    if db_entry is None:
        raise NotFoundError(detail="Journal entry not found.")
    return db_entry

@app.delete("/journal-entries/{entry_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("accounting.delete.journal_entries"))])
async def delete_existing_journal_entry(
    entry_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_journal_entry(db_session, user_id, entry_id)
    if not success:
        raise NotFoundError(detail="Journal entry not found.")
    return {"ok": True}

# --- Ledger Endpoints ---
# ... (unchanged) ...

# --- Trial Balance Endpoints ---
# ... (unchanged) ...

# --- Financial Statement Endpoints ---
# ... (unchanged) ...

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Accounting Service is running!"}
