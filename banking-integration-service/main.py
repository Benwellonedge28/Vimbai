from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from banking_integration_service import models, crud
from banking_integration_service.database import init_db_schema, Neo4jConnector
from banking_integration_service.dependencies import get_db_session, get_user_id, get_jwt_token
from banking_integration_service.utils.auth import check_permission
from banking_integration_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv
from pydantic import ValidationError as PydanticValidationError

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Banking Integration Service",
    description="Connects to various banks, retrieves transaction data, and integrates with the Accounting Service.",
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

# --- Bank Endpoints ---
# ... (unchanged) ...

# --- Bank Account Endpoints ---
# ... (unchanged) ...

# --- Transaction Endpoints ---
@app.post("/transactions/", response_model=models.TransactionInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.transactions"))])
async def create_new_transaction(
    transaction: models.TransactionCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token), # Pass JWT for internal calls
    db_session: AsyncSession = Depends(get_db_session)
):
    db_bank_account = await crud.get_bank_account(db_session, transaction.bank_account_id, user_id)
    if not db_bank_account:
        raise ValidationError(detail="Bank account not found for this user.", code="BANK_ACCOUNT_NOT_FOUND")
    
    # Check if a transaction with the same bank-provided transaction_id already exists for this account
    existing_transactions = await crud.get_all_transactions_for_bank_account(db_session, transaction.bank_account_id, user_id)
    for existing_t in existing_transactions:
        if existing_t.transaction_id == transaction.transaction_id:
            raise ConflictError(detail="Transaction with this ID already exists for this account.", code="TRANSACTION_EXISTS")

    return await crud.create_transaction(db_session, user_id, transaction, jwt_token)

@app.get("/transactions/{transaction_id}", response_model=models.TransactionInDB,
             dependencies=[Depends(check_permission("banking.read.transactions"))])
async def read_transaction_by_id(
    transaction_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_transaction = await crud.get_transaction(db_session, transaction_id, user_id)
    if db_transaction is None:
        raise NotFoundError(detail="Transaction not found.")
    return db_transaction
    
@app.get("/transactions/"), response_model=List[models.TransactionInDB],
             dependencies=[Depends(check_permission("banking.read.transactions"))])
async def read_all_transactions(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    bank_account_id: Optional[str] = Query(None, description="Filter transactions by bank account ID."),
    start_date: Optional[datetime] = Query(None, description="Start date for filtering transactions."),
    end_date: Optional[datetime] = Query(None, description="End date for filtering transactions.")
):
    if bank_account_id:
        db_bank_account = await crud.get_bank_account(db_session, bank_account_id, user_id)
        if not db_bank_account:
            raise NotFoundError(detail="Bank account not found for this user.", code="BANK_ACCOUNT_NOT_FOUND")
        return await crud.get_all_transactions_for_bank_account(db_session, bank_account_id, user_id, start_date, end_date)
    return await crud.get_all_transactions(db_session, user_id, start_date, end_date)

@app.put("/transactions/{transaction_id}", response_model=models.TransactionInDB,
             dependencies=[Depends(check_permission("banking.write.transactions"))])
async def update_existing_transaction(
    transaction_id: str,
    transaction: models.TransactionUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_transaction = await crud.update_transaction(db_session, transaction_id, user_id, transaction)
    if db_transaction is None:
        raise NotFoundError(detail="Transaction not found.")
    return db_transaction

@app.delete("/transactions/{transaction_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("banking.delete.transactions"))])
async def delete_existing_transaction(
    transaction_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_transaction(db_session, transaction_id, user_id)
    if not success:
        raise NotFoundError(detail="Transaction not found.")
    return {"ok": True}

@app.post("/transactions/{transaction_id}/create-journal-entry", response_model=models.CreateJournalEntryResponse,
              dependencies=[Depends(check_permission("banking.create.journal_entry"))])
async def create_journal_entry_for_transaction(
    transaction_id: str,
    debit_account_number: str = Query(..., description="The account to debit (e.g., Expense account)"),
    credit_account_number: str = Query(..., description="The account to credit (e.g., Cash or Accounts Payable)"),
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token), # Pass JWT for internal calls
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.analyze_transaction_and_create_journal_entry(
        db_session, transaction_id, user_id, debit_account_number, credit_account_number, jwt_token
    )

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Banking Integration Service is running!"}
