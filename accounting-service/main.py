from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from accounting_service import models, crud
from accounting_service.database import init_db_schema, Neo4jConnector
from accounting_service.dependencies import get_db_session
from accounting_service.utils.auth import check_permission
from accounting_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv
from pydantic import ValidationError as PydanticValidationError # NEW: to catch pydantic's internal validation errors

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Accounting Service",
    description="Manages Chart of Accounts, Journal Entries, Ledgers, and Financial Statements.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
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
async def create_new_journal_entry(entry: models.JournalEntryCreate, db_session: AsyncSession = Depends(get_db_session)):
    for line in entry.lines:
        account = await crud.get_account_by_number(db_session, line.account_number)
        if not account:
            raise ValidationError(detail=f"Account number {line.account_number} in journal line does not exist.", code="ACCOUNT_NOT_FOUND_IN_JE")
    return await crud.create_journal_entry(db_session, entry)

@app.get("/journal-entries/{entry_id}", response_model=models.JournalEntryInDB,
             dependencies=[Depends(check_permission("accounting.read.journal_entries"))])
async def read_journal_entry_by_id(entry_id: str, db_session: AsyncSession = Depends(get_db_session)):
    db_entry = await crud.get_journal_entry(db_session, entry_id)
    if db_entry is None:
        raise NotFoundError(detail="Journal entry not found.")
    return db_entry
        
@app.get("/journal-entries/")
async def read_all_journal_entries(db_session: AsyncSession = Depends(get_db_session), 
                           _=Depends(check_permission("accounting.read.journal_entries"))) -> List[models.JournalEntryInDB]:
    return await crud.get_all_journal_entries(db_session)

@app.put("/journal-entries/{entry_id}/status", response_model=models.JournalEntryInDB,
             dependencies=[Depends(check_permission("accounting.write.journal_entries_status"))]) # NEW permission
async def update_journal_entry_status(
    entry_id: str,
    new_status: models.JournalEntryUpdate, # Expects a model containing only status for now
    db_session: AsyncSession = Depends(get_db_session)
):
    db_entry = await crud.get_journal_entry(db_session, entry_id)
    if db_entry is None:
        raise NotFoundError(detail="Journal entry not found.")
    
    updated_entry = await crud.update_journal_entry(db_session, entry_id, new_status)
    if updated_entry is None:
        raise HTTPException(status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail="Failed to update journal entry status.")
    return updated_entry

@app.delete("/journal-entries/{entry_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("accounting.delete.journal_entries"))])
async def delete_existing_journal_entry(entry_id: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_journal_entry(db_session, entry_id)
    if not success:
        raise NotFoundError(detail="Journal entry not found.")
    return {"ok": True}

# ... (existing Ledger, Trial Balance, Financial Statements Endpoints) ...
