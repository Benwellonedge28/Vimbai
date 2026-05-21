from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from banking_integration_service import models, crud
from banking_integration_service.database import init_db_schema, Neo4jConnector
from banking_integration_service.dependencies import get_db_session, get_user_id # Assuming these are defined
from banking_integration_service.utils.auth import check_permission # Assuming check_permission is defined
from banking_integration_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Banking Integration Service",
    description="Manages bank connections, synchronizes transactions, and facilitates reconciliation.",
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
    await init_db_schema() # Initialize Neo4j schema specific to banking integration service

@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()

# --- Global Exception Handlers (placeholders) ---
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

# --- Bank Connection Endpoints ---
@app.post("/connections/", response_model=models.BankConnectionInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.connections"))])
async def create_bank_connection(
    connection: models.BankConnectionCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    # Ensure the user_id from token matches the one in the request or override it
    connection.user_id = user_id
    return await crud.create_bank_connection(db_session, connection)

@app.get("/connections/", response_model=List[models.BankConnectionInDB],
             dependencies=[Depends(check_permission("banking.read.connections"))])
async def read_all_bank_connections(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_bank_connections(db_session, user_id)

@app.get("/connections/{connection_id}", response_model=models.BankConnectionInDB,
             dependencies=[Depends(check_permission("banking.read.connections"))])
async def read_bank_connection_by_id(
    connection_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    connection = await crud.get_bank_connection(db_session, user_id, connection_id)
    if connection is None:
        raise NotFoundError(detail="Bank Connection not found.")
    return connection

@app.put("/connections/{connection_id}", response_model=models.BankConnectionInDB,
             dependencies=[Depends(check_permission("banking.write.connections"))])
async def update_bank_connection(
    connection_id: str,
    connection: models.BankConnectionUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_connection = await crud.update_bank_connection(db_session, user_id, connection_id, connection)
    if updated_connection is None:
        raise NotFoundError(detail="Bank Connection not found.")
    return updated_connection

@app.delete("/connections/{connection_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("banking.delete.connections"))])
async def delete_bank_connection(
    connection_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_bank_connection(db_session, user_id, connection_id)
    if not success:
        raise NotFoundError(detail="Bank Connection not found.")
    return {"ok": True}

# --- Bank Account Endpoints ---
@app.post("/connections/{connection_id}/accounts/", response_model=models.BankAccountInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.accounts"))])
async def create_bank_account(
    connection_id: str,
    account: models.BankAccountCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    # This assumes account.connection_id matches the path parameter connection_id
    # or that connection_id is set internally
    account.connection_id = connection_id
    return await crud.create_bank_account(db_session, connection_id, account)

@app.get("/connections/{connection_id}/accounts/", response_model=List[models.BankAccountInDB],
             dependencies=[Depends(check_permission("banking.read.accounts"))])
async def read_bank_accounts_for_connection(
    connection_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_bank_accounts_for_connection(db_session, connection_id)

@app.get("/accounts/{account_id}", response_model=models.BankAccountInDB,
             dependencies=[Depends(check_permission("banking.read.accounts"))])
async def read_bank_account_by_id(
    account_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    account = await crud.get_bank_account(db_session, account_id)
    if account is None:
        raise NotFoundError(detail="Bank Account not found.")
    return account

@app.put("/accounts/{account_id}", response_model=models.BankAccountInDB,
             dependencies=[Depends(check_permission("banking.write.accounts"))])
async def update_bank_account(
    account_id: str,
    account: models.BankAccountUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_account = await crud.update_bank_account(db_session, account_id, account)
    if updated_account is None:
        raise NotFoundError(detail="Bank Account not found.")
    return updated_account

@app.delete("/accounts/{account_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("banking.delete.accounts"))])
async def delete_bank_account(
    account_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_bank_account(db_session, account_id)
    if not success:
        raise NotFoundError(detail="Bank Account not found.")
    return {"ok": True}

# --- Bank Transaction Endpoints ---
@app.post("/accounts/{account_id}/transactions/", response_model=models.BankTransactionInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.transactions"))])
async def create_bank_transaction(
    account_id: str,
    transaction: models.BankTransactionCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    transaction.account_id = account_id
    return await crud.create_bank_transaction(db_session, account_id, transaction)

@app.get("/accounts/{account_id}/transactions/", response_model=List[models.BankTransactionInDB],
             dependencies=[Depends(check_permission("banking.read.transactions"))])
async def read_bank_transactions_for_account(
    account_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_bank_transactions_for_account(db_session, account_id)

@app.get("/transactions/{transaction_id}", response_model=models.BankTransactionInDB,
             dependencies=[Depends(check_permission("banking.read.transactions"))])
async def read_bank_transaction_by_id(
    transaction_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    transaction = await crud.get_bank_transaction(db_session, transaction_id)
    if transaction is None:
        raise NotFoundError(detail="Bank Transaction not found.")
    return transaction

@app.put("/transactions/{transaction_id}", response_model=models.BankTransactionInDB,
             dependencies=[Depends(check_permission("banking.write.transactions"))])
async def update_bank_transaction(
    transaction_id: str,
    transaction: models.BankTransactionUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_transaction = await crud.update_bank_transaction(db_session, transaction_id, transaction)
    if updated_transaction is None:
        raise NotFoundError(detail="Bank Transaction not found.")
    return updated_transaction

# --- Transaction Categorization Rule Endpoints ---
@app.post("/categorization-rules/", response_model=models.TransactionCategorizationRuleInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.rules"))])
async def create_categorization_rule(
    rule: models.TransactionCategorizationRuleCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    rule.user_id = user_id
    return await crud.create_categorization_rule(db_session, user_id, rule)

@app.get("/categorization-rules/", response_model=List[models.TransactionCategorizationRuleInDB],
             dependencies=[Depends(check_permission("banking.read.rules"))])
async def read_all_categorization_rules(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_categorization_rules(db_session, user_id)

@app.get("/categorization-rules/{rule_id}", response_model=models.TransactionCategorizationRuleInDB,
             dependencies=[Depends(check_permission("banking.read.rules"))])
async def read_categorization_rule_by_id(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    rule = await crud.get_categorization_rule(db_session, user_id, rule_id)
    if rule is None:
        raise NotFoundError(detail="Transaction Categorization Rule not found.")
    return rule

@app.put("/categorization-rules/{rule_id}", response_model=models.TransactionCategorizationRuleInDB,
             dependencies=[Depends(check_permission("banking.write.rules"))])
async def update_categorization_rule(
    rule_id: str,
    rule: models.TransactionCategorizationRuleUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_rule = await crud.update_categorization_rule(db_session, user_id, rule_id, rule)
    if updated_rule is None:
        raise NotFoundError(detail="Transaction Categorization Rule not found.")
    return updated_rule

@app.delete("/categorization-rules/{rule_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("banking.delete.rules"))])
async def delete_categorization_rule(
    rule_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_categorization_rule(db_session, user_id, rule_id)
    if not success:
        raise NotFoundError(detail="Transaction Categorization Rule not found.")
    return {"ok": True}

# --- Reconciliation Match Endpoints ---
@app.post("/reconciliation-matches/", response_model=models.ReconciliationMatchInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.reconciliation"))])
async def create_reconciliation_match(
    match: models.ReconciliationMatchCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_reconciliation_match(db_session, match)

@app.get("/reconciliation-matches/{match_id}", response_model=models.ReconciliationMatchInDB,
             dependencies=[Depends(check_permission("banking.read.reconciliation"))])
async def read_reconciliation_match_by_id(
    match_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    match = await crud.get_reconciliation_match(db_session, match_id)
    if match is None:
        raise NotFoundError(detail="Reconciliation Match not found.")
    return match

@app.put("/reconciliation-matches/{match_id}", response_model=models.ReconciliationMatchInDB,
             dependencies=[Depends(check_permission("banking.write.reconciliation"))])
async def update_reconciliation_match(
    match_id: str,
    match: models.ReconciliationMatchUpdate,
    db_session: AsyncSession = Depends(get_db_session)
):
    updated_match = await crud.update_reconciliation_match(db_session, match_id, match)
    if updated_match is None:
        raise NotFoundError(detail="Reconciliation Match not found.")
    return updated_match

@app.delete("/reconciliation-matches/{match_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("banking.delete.reconciliation"))])
async def delete_reconciliation_match(
    match_id: str,
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_reconciliation_match(db_session, match_id)
    if not success:
        raise NotFoundError(detail="Reconciliation Match not found.")
    return {"ok": True}

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Banking Integration Service is running!"}
