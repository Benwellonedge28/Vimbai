from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse
from typing import List, Optional, Dict # Added Dict for response model
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
from decimal import Decimal # Added Decimal for response type

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
        error_details.append(f"Field '{loc}': {error['msg']}")
    return JSONResponse(
        status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
        content={"detail": "Validation error: " + "; ".join(error_details), "code": "PYDANTIC_VALIDATION_ERROR"},
    )

# --- Chart of Accounts Endpoints ---
@app.post("/accounts/", response_model=models.AccountInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("accounting.write.accounts"))])
async def create_new_account(
    account: models.AccountCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_account(db_session, user_id, account)

@app.get("/accounts/{account_number}", response_model=models.AccountInDB,
             dependencies=[Depends(check_permission("accounting.read.accounts"))])
async def read_account_by_number(
    account_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_account = await crud.get_account(db_session, user_id, account_number)
    if db_account is None:
        raise NotFoundError(detail="Account not found.")
    return db_account

@app.get("/accounts/", response_model=List[models.AccountInDB],
             dependencies=[Depends(check_permission("accounting.read.accounts"))])
async def read_all_accounts(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_accounts(db_session, user_id)

@app.put("/accounts/{account_number}", response_model=models.AccountInDB,
             dependencies=[Depends(check_permission("accounting.write.accounts"))])
async def update_existing_account(
    account_number: str,
    account: models.AccountUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_account = await crud.update_account(db_session, user_id, account_number, account)
    if db_account is None:
        raise NotFoundError(detail="Account not found.")
    return db_account

@app.delete("/accounts/{account_number}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("accounting.delete.accounts"))])
async def delete_existing_account(
    account_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_account(db_session, user_id, account_number)
    if not success:
        raise NotFoundError(detail="Account not found or linked to existing entries.")
    return {"ok": True}

# --- NEW: Endpoint to get account activity for a period (for Budget Variance Report) ---
@app.get("/accounts/{account_number}/period-activity", response_model=Dict[str, Decimal],
             dependencies=[Depends(check_permission("accounting.read.accounts"))])
async def get_account_activity_for_period(
    account_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: datetime = Query(..., description="Start date for the period (ISO format)."),
    end_date: datetime = Query(..., description="End date for the period (ISO format).")
):
    total_debits, total_credits = await crud.get_account_period_activity(db_session, user_id, account_number, start_date, end_date)
    return {"total_debits": total_debits, "total_credits": total_credits}


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

# --- Vendor Bill Endpoints (NEW) ---
@app.post("/vendor-bills/", response_model=models.VendorBillInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("accounting.create.vendor_bill"))])
async def create_vendor_bill(
    vendor_bill: models.VendorBillCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_vendor_bill(db_session, user_id, vendor_bill, jwt_token)

# --- Ledger Endpoints ---
@app.get("/ledgers/{account_number}", response_model=models.LedgerReport,
             dependencies=[Depends(check_permission("accounting.read.ledgers"))])
async def get_account_ledger(
    account_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None, description="Start date for ledger entries (ISO format)."),
    end_date: Optional[datetime] = Query(None, description="End date for ledger entries (ISO format).")
):
    return await crud.get_ledger_report(db_session, user_id, account_number, start_date, end_date)

# --- Trial Balance Endpoints ---
@app.get("/trial-balance/", response_model=models.TrialBalanceReport,
             dependencies=[Depends(check_permission("accounting.read.trial_balance"))])
async def get_current_trial_balance(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None, description="Date to generate trial balance as of (ISO format).")
):
    return await crud.get_trial_balance_report(db_session, user_id, as_of_date)

# --- Income Statement Endpoints ---
@app.get("/income-statement/", response_model=models.IncomeStatement,
             dependencies=[Depends(check_permission("accounting.read.income_statement"))])
async def get_income_statement_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: datetime = Query(..., description="Start date for the income statement period (ISO format)."),
    end_date: datetime = Query(..., description="End date for the income statement period (ISO format).")
):
    return await crud.get_income_statement(db_session, user_id, start_date, end_date)

# --- Balance Sheet Endpoints ---
@app.get("/balance-sheet/", response_model=models.BalanceSheet,
             dependencies=[Depends(check_permission("accounting.read.balance_sheet"))])
async def get_balance_sheet_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: datetime = Query(..., description="Date to generate the balance sheet as of (ISO format).")
):
    return await crud.get_balance_sheet(db_session, user_id, as_of_date)

# --- Sales Journal Endpoints ---
@app.post("/sales-journal/", response_model=models.SalesJournalEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.sales_journal"))])
async def create_sales_journal_entry(
    entry: models.SalesJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_sales_journal_entry(db_session, user_id, entry, jwt_token)

@app.get("/sales-journal/", response_model=List[models.SalesJournalEntryInDB],
         dependencies=[Depends(check_permission("accounting.read.sales_journal"))])
async def read_sales_journal_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None, description="Start date for filtering"),
    end_date: Optional[datetime] = Query(None, description="End date for filtering"),
    customer_id: Optional[str] = Query(None, description="Filter by customer ID"),
    status: Optional[str] = Query(None, description="Filter by status")
):
    return await crud.get_sales_journal_entries(db_session, user_id, start_date, end_date, customer_id, status)

@app.get("/sales-journal/{entry_id}", response_model=models.SalesJournalEntryInDB,
         dependencies=[Depends(check_permission("accounting.read.sales_journal"))])
async def read_sales_journal_entry(
    entry_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_sales_journal_entry(db_session, user_id, entry_id)

# --- Purchases Journal Endpoints ---
@app.post("/purchases-journal/", response_model=models.PurchasesJournalEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.purchases_journal"))])
async def create_purchases_journal_entry(
    entry: models.PurchasesJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_purchases_journal_entry(db_session, user_id, entry, jwt_token)

@app.get("/purchases-journal/", response_model=List[models.PurchasesJournalEntryInDB],
         dependencies=[Depends(check_permission("accounting.read.purchases_journal"))])
async def read_purchases_journal_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None),
    vendor_id: Optional[str] = Query(None),
    status: Optional[str] = Query(None)
):
    return await crud.get_purchases_journal_entries(db_session, user_id, start_date, end_date, vendor_id, status)

# --- Cash Receipts Journal Endpoints ---
@app.post("/cash-receipts-journal/", response_model=models.CashReceiptsJournalEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.cash_receipts"))])
async def create_cash_receipts_entry(
    entry: models.CashReceiptsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_cash_receipts_entry(db_session, user_id, entry, jwt_token)

@app.get("/cash-receipts-journal/", response_model=List[models.CashReceiptsJournalEntryInDB],
         dependencies=[Depends(check_permission("accounting.read.cash_receipts"))])
async def read_cash_receipts_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None)
):
    return await crud.get_cash_receipts_entries(db_session, user_id, start_date, end_date)

# --- Cash Disbursements Journal Endpoints ---
@app.post("/cash-disbursements-journal/", response_model=models.CashDisbursementsJournalEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.cash_disbursements"))])
async def create_cash_disbursements_entry(
    entry: models.CashDisbursementsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_cash_disbursements_entry(db_session, user_id, entry, jwt_token)

@app.get("/cash-disbursements-journal/", response_model=List[models.CashDisbursementsJournalEntryInDB],
         dependencies=[Depends(check_permission("accounting.read.cash_disbursements"))])
async def read_cash_disbursements_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None)
):
    return await crud.get_cash_disbursements_entries(db_session, user_id, start_date, end_date)

# --- Sales Returns Journal Endpoints ---
@app.post("/sales-returns-journal/", response_model=models.SalesReturnsJournalEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.sales_returns"))])
async def create_sales_return_entry(
    entry: models.SalesReturnsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_sales_return_entry(db_session, user_id, entry, jwt_token)

# --- Purchases Returns Journal Endpoints ---
@app.post("/purchases-returns-journal/", response_model=models.PurchasesReturnsJournalEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.purchases_returns"))])
async def create_purchases_return_entry(
    entry: models.PurchasesReturnsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_purchases_return_entry(db_session, user_id, entry, jwt_token)

# --- Subsidiary Ledgers Endpoints ---
@app.get("/subsidiary-ledgers/accounts-receivable/", response_model=models.AccountsReceivableLedgerReport,
         dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))])
async def get_ar_ledger_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None, description="Date for AR aging"),
    customer_id: Optional[str] = Query(None, description="Filter by customer")
):
    return await crud.get_ar_ledger_report(db_session, user_id, as_of_date, customer_id)

@app.get("/subsidiary-ledgers/accounts-payable/", response_model=models.AccountsPayableLedgerReport,
         dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))])
async def get_ap_ledger_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None, description="Date for AP aging"),
    vendor_id: Optional[str] = Query(None, description="Filter by vendor")
):
    return await crud.get_ap_ledger_report(db_session, user_id, as_of_date, vendor_id)

@app.get("/subsidiary-ledgers/fixed-assets/", response_model=models.FixedAssetsLedgerReport,
         dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))])
async def get_fixed_assets_ledger(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None),
    status: Optional[str] = Query(None, description="Filter by asset status")
):
    return await crud.get_fixed_assets_ledger(db_session, user_id, as_of_date, status)

@app.get("/subsidiary-ledgers/inventory/", response_model=models.InventoryLedgerReport,
         dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))])
async def get_inventory_ledger(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None),
    category: Optional[str] = Query(None, description="Filter by category")
):
    return await crud.get_inventory_ledger(db_session, user_id, as_of_date, category)

# --- Petty Cash Endpoints ---
@app.post("/petty-cash-funds/", response_model=models.PettyCashFundInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.petty_cash"))])
async def create_petty_cash_fund(
    fund: models.PettyCashFundCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_petty_cash_fund(db_session, user_id, fund)

@app.get("/petty-cash-funds/", response_model=List[models.PettyCashFundInDB],
         dependencies=[Depends(check_permission("accounting.read.petty_cash"))])
async def read_petty_cash_funds(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_petty_cash_funds(db_session, user_id)

@app.post("/petty-cash-entries/", response_model=models.PettyCashEntryInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.petty_cash"))])
async def create_petty_cash_entry(
    entry: models.PettyCashEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_petty_cash_entry(db_session, user_id, entry, jwt_token)

@app.get("/petty-cash-entries/", response_model=List[models.PettyCashEntryInDB],
         dependencies=[Depends(check_permission("accounting.read.petty_cash"))])
async def read_petty_cash_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    fund_id: Optional[str] = Query(None, description="Filter by fund ID"),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None)
):
    return await crud.get_petty_cash_entries(db_session, user_id, fund_id, start_date, end_date)

# --- Bank Reconciliation Endpoints ---
@app.post("/bank-reconciliation/", response_model=models.BankReconciliationStatement, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("accounting.write.bank_reconciliation"))])
async def create_bank_reconciliation(
    bank_account: str,
    statement_date: datetime,
    statement_balance: Decimal,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_bank_reconciliation(db_session, user_id, bank_account, statement_date, statement_balance, jwt_token)

@app.get("/bank-reconciliation/", response_model=List[models.BankReconciliationStatement],
         dependencies=[Depends(check_permission("accounting.read.bank_reconciliation"))])
async def get_bank_reconciliations(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    bank_account: Optional[str] = Query(None),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None)
):
    return await crud.get_bank_reconciliations(db_session, user_id, bank_account, start_date, end_date)

@app.get("/bank-reconciliation/latest/{bank_account}", response_model=models.BankReconciliationStatement,
         dependencies=[Depends(check_permission("accounting.read.bank_reconciliation"))])
async def get_latest_reconciliation(
    bank_account: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_latest_bank_reconciliation(db_session, user_id, bank_account)
