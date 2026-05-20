from fastapi import FastAPI, Depends, HTTPException, status
from typing import List, Optional
from neo4j import AsyncSession
from accounting_service import models, crud
from accounting_service.database import init_db_schema, Neo4jConnector
from accounting_service.dependencies import get_db_session
from accounting_service.utils.auth import check_permission
import os
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Accounting Service",
    description="Manages Chart of Accounts, Journal Entries, Ledgers, and Financial Statements.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.get_driver() # Initialize driver
    await init_db_schema() # Ensure schema and constraints

@app.on_event("shutdown")
async def shutdown_event():
    Neo4jConnector.close_driver() # Close driver

# --- Chart of Accounts Endpoints ---

@app.post("/accounts/", response_model=models.AccountInDB, status_code=status.HTTP_201_CREATED, 
          dependencies=[Depends(check_permission("accounting.write.accounts"))])
async def create_new_account(account: models.AccountCreate, db_session: AsyncSession = Depends(get_db_session)):
    db_account = await crud.get_account_by_number(db_session, account.account_number)
    if db_account:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Account with this number already exists")
    return await crud.create_account(db_session, account)

@app.get("/accounts/{account_number}", response_model=models.AccountInDB, 
         dependencies=[Depends(check_permission("accounting.read.accounts"))])
async def read_account_by_number(account_number: str, db_session: AsyncSession = Depends(get_db_session)):
    db_account = await crud.get_account_by_number(db_session, account_number)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Account not found")
    return db_account
    
@app.get("/accounts/", response_model=List[models.AccountInDB], 
         dependencies=[Depends(check_permission("accounting.read.accounts"))])
async def read_all_accounts(db_session: AsyncSession = Depends(get_db_session)): # Removed explicit return type hint to avoid IDE errors with List[...]
    return await crud.get_all_accounts(db_session)

@app.put("/accounts/{account_number}", response_model=models.AccountInDB, 
         dependencies=[Depends(check_permission("accounting.write.accounts"))])
async def update_existing_account(account_number: str, account: models.AccountUpdate, db_session: AsyncSession = Depends(get_db_session)):
    db_account = await crud.update_account(db_session, account_number, account)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Account not found")
    return db_account

@app.delete("/accounts/{account_number}", status_code=status.HTTP_204_NO_CONTENT, 
            dependencies=[Depends(check_permission("accounting.delete.accounts"))])
async def delete_existing_account(account_number: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_account(db_session, account_number)
    if not success:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Account not found")
    return {"ok": True}

# --- Journal Entry Endpoints ---

@app.post("/journal-entries/", response_model=models.JournalEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.journal_entries"))])
async def create_new_journal_entry(entry: models.JournalEntryCreate, db_session: AsyncSession = Depends(get_db_session)):
    # Basic validation: check if all accounts in lines exist
    for line in entry.lines:
        account = await crud.get_account_by_number(db_session, line.account_number)
        if not account:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Account number {line.account_number} in journal line does not exist."
            )
    return await crud.create_journal_entry(db_session, entry)

@app.get("/journal-entries/{entry_id}", response_model=models.JournalEntryInDB,
         dependencies=[Depends(check_permission("accounting.read.journal_entries"))])
async def read_journal_entry_by_id(entry_id: str, db_session: AsyncSession = Depends(get_db_session)):
    db_entry = await crud.get_journal_entry(db_session, entry_id)
    if db_entry is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Journal entry not found")
    return db_entry

@app.get("/journal-entries/")
async def read_all_journal_entries(db_session: AsyncSession = Depends(get_db_session), 
                           _=Depends(check_permission("accounting.read.journal_entries"))) -> List[models.JournalEntryInDB]:
    return await crud.get_all_journal_entries(db_session)

@app.delete("/journal-entries/{entry_id}", status_code=status.HTTP_204_NO_CONTENT,
            dependencies=[Depends(check_permission("accounting.delete.journal_entries"))])
async def delete_existing_journal_entry(entry_id: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_journal_entry(db_session, entry_id)
    if not success:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Journal entry not found")
    return {"ok": True}

# --- Ledger and Trial Balance Endpoints (NEW) ---

@app.get("/ledger/{account_number}", response_model=models.LedgerAccountBalance,
         dependencies=[Depends(check_permission("accounting.read.ledger"))])
async def get_ledger_for_account(account_number: str, db_session: AsyncSession = Depends(get_db_session)):
    balance = await crud.get_ledger_account_balance(db_session, account_number)
    if balance is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Account not found or no transactions")
    return balance

@app.get("/trial-balance/", response_model=models.TrialBalance,
         dependencies=[Depends(check_permission("accounting.read.trial_balance"))])
async def get_current_trial_balance(db_session: AsyncSession = Depends(get_db_session)): # Removed explicit return type hint to avoid IDE errors with List[...]
    trial_balance = await crud.generate_trial_balance(db_session)
    if not trial_balance.entries:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="No accounts or transactions found to generate trial balance")
    return trial_balance

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Accounting Service is running!"}
