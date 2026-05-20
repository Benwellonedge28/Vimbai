from fastapi import FastAPI, Depends, HTTPException, status
from typing import List, Optional
from neo4j import AsyncSession
from accounting_service import models, crud
from accounting_service.database import init_db_schema, Neo4jConnector
from accounting_service.dependencies import get_db_session
from accounting_service.utils.auth import check_permission # NEW
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
          dependencies=[Depends(check_permission("accounting.write.accounts"))]) # NEW: Add permission check
async def create_new_account(account: models.AccountCreate, db_session: AsyncSession = Depends(get_db_session)):
    db_account = await crud.get_account_by_number(db_session, account.account_number)
    if db_account:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Account with this number already exists")
    return await crud.create_account(db_session, account)

@app.get("/accounts/{account_number}", response_model=models.AccountInDB, 
         dependencies=[Depends(check_permission("accounting.read.accounts"))]) # NEW: Add permission check
async def read_account_by_number(account_number: str, db_session: AsyncSession = Depends(get_db_session)):
    db_account = await crud.get_account_by_number(db_session, account_number)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Account not found")
    return db_account
    
@app.get("/accounts/")
async def read_all_accounts(db_session: AsyncSession = Depends(get_db_session), 
                           _=Depends(check_permission("accounting.read.accounts"))) -> List[models.AccountInDB]: # NEW: Add permission check
    return await crud.get_all_accounts(db_session)

@app.put("/accounts/{account_number}", response_model=models.AccountInDB, 
         dependencies=[Depends(check_permission("accounting.write.accounts"))]) # NEW: Add permission check
async def update_existing_account(account_number: str, account: models.AccountUpdate, db_session: AsyncSession = Depends(get_db_session)):
    db_account = await crud.update_account(db_session, account_number, account)
    if db_account is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Account not found")
    return db_account

@app.delete("/accounts/{account_number}", status_code=status.HTTP_204_NO_CONTENT, 
            dependencies=[Depends(check_permission("accounting.delete.accounts"))]) # NEW: Add permission check
async def delete_existing_account(account_number: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_account(db_session, account_number)
    if not success:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Account not found")
    return {"ok": True}

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Accounting Service is running!"}
