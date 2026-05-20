from fastapi import FastAPI, Depends, HTTPException, status
from typing import List, Optional
from neo4j import AsyncSession
from banking_integration_service import models, crud
from banking_integration_service.database import init_db_schema, Neo4jConnector
from banking_integration_service.dependencies import get_db_session, get_user_id
from banking_integration_service.utils.auth import check_permission
import os
from dotenv import load_dotenv
from datetime import datetime

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Banking Integration Service",
    description="Manages connected bank accounts and fetches transactions.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.get_driver() # Initialize driver
    await init_db_schema() # Ensure schema and constraints

@app.on_event("shutdown")
async def shutdown_event():
    Neo4jConnector.close_driver() # Close driver

# --- Bank Account Endpoints ---
@app.post("/bank-accounts/", response_model=models.BankAccountInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.accounts"))])
async def create_new_bank_account(
    account: models.BankAccountCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_account = await crud.get_bank_account_by_id(db_session, account.account_id, user_id)
    if db_account:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Bank account with this ID already exists for user.")
    return await crud.create_bank_account(db_session, user_id, account)

@app.get("/bank-accounts/", response_model=List[models.BankAccountInDB],
             dependencies=[Depends(check_permission("banking.read.accounts"))])
async def read_all_bank_accounts(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_bank_accounts(db_session, user_id)

@app.get("/bank-accounts/{account_id}", response_model=models.BankAccountInDB,
             dependencies=[Depends(check_permission("banking.read.accounts"))])
async def read_bank_account_by_id(
    account_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_account = await crud.get_bank_account_by_id(db_session, account_id, user_id)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bank account not found.")
    return db_account
    
@app.put("/bank-accounts/{account_id}", response_model=models.BankAccountInDB,
             dependencies=[Depends(check_permission("banking.write.accounts"))])
async def update_existing_bank_account(
    account_id: str,
    account: models.BankAccountUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_account = await crud.update_bank_account(db_session, account_id, user_id, account)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bank account not found.")
    return db_account

@app.delete("/bank-accounts/{account_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("banking.delete.accounts"))])
async def delete_existing_bank_account(
    account_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_bank_account(db_session, account_id, user_id)
    if not success:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bank account not found.")
    return {"ok": True}

# --- Bank Transaction Endpoints ---
@app.post("/bank-accounts/{bank_account_id}/fetch-transactions", response_model=List[models.BankTransactionInDB],
              dependencies=[Depends(check_permission("banking.fetch.transactions"))])
async def fetch_and_store_transactions(
    bank_account_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    # First, retrieve the bank account to get its internal Neo4j ID
    db_account = await crud.get_bank_account_by_id(db_session, bank_account_id, user_id)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bank account not found.")
    
    # Mock fetching transactions from an external bank API
    mock_transactions = await crud.mock_fetch_external_transactions(bank_account_id, user_id)
    
    stored_transactions = []
    for transaction in mock_transactions:
        # Check if transaction already exists (by transaction_id for the account) to prevent duplicates
        existing_tx = await crud.get_bank_transaction_by_id(db_session, transaction.transaction_id, user_id)
        if not existing_tx: # Only store new transactions
            stored_tx = await crud.create_bank_transaction(db_session, db_account.id, transaction)
            stored_transactions.append(stored_tx)
    
    # Update last synced time for the bank account
    await crud.update_bank_account(db_session, bank_account_id, user_id, models.BankAccountUpdate(last_synced_at=datetime.utcnow(), is_synced=True))

    return stored_transactions

@app.get("/bank-accounts/{bank_account_id}/transactions", response_model=List[models.BankTransactionInDB],
             dependencies=[Depends(check_permission("banking.read.transactions"))])
async def get_transactions_for_bank_account(
    bank_account_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_account = await crud.get_bank_account_by_id(db_session, bank_account_id, user_id)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Bank account not found.")
    
    return await crud.get_bank_transactions_for_account(db_session, bank_account_id, user_id)

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Banking Integration Service is running!"}
