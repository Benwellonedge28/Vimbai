import os
from datetime import datetime
from decimal import Decimal  # Added Decimal for response type
from typing import Dict, List, Optional  # Added Dict for response model

from accounting_service import crud, models
from accounting_service.database import Neo4jConnector
from accounting_service.dependencies import book_id_var, get_db_session, get_jwt_token, get_user_id
from accounting_service.exceptions import (
    ConflictError,
    ForbiddenError,
    NotFoundError,
    UnauthorizedError,
    ValidationError,
)
from accounting_service.utils.auth import check_permission
from dotenv import load_dotenv
from fastapi import Depends, FastAPI, HTTPException, Query, Request, status
from fastapi.openapi.utils import get_openapi
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from pydantic import ValidationError as PydanticValidationError

# Load environment variables
load_dotenv()

# =============================================================================
# OPENAPI SCHEMA CONFIGURATION
# =============================================================================


def custom_openapi():
    """Generate custom OpenAPI schema with Vimbai-specific metadata."""
    if app.openapi_schema:
        return app.openapi_schema

    openapi_schema = get_openapi(
        title="Vimbai Accounting Service",
        description="""
## Vimbai Accounting Service API

Comprehensive accounting API covering double-entry bookkeeping, special journals,
subsidiary ledgers, and financial reporting.

### Features
- **Chart of Accounts**: Complete account hierarchy management
- **Journal Entries**: Double-entry transaction recording
- **Ledger Reports**: Account activity and balance tracking
- **Financial Statements**: Trial balance, income statement, balance sheet
- **Special Journals**: Sales, purchases, cash receipts/disbursements
- **Subsidiary Ledgers**: AR/AP aging, fixed assets, inventory
- **Petty Cash**: Fund management and tracking
- **Bank Reconciliation**: Statement matching and verification
- **Incomplete Records**: Single-entry accounting support

### Authentication
All endpoints require JWT Bearer token authentication.

### Rate Limits
- Default: 1000 requests/minute
- Authenticated: 5000 requests/minute
        """,
        version="1.0.0",
        routes=app.routes,
    )

    # Add security schemes
    openapi_schema["components"]["securitySchemes"] = {
        "BearerAuth": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT", "description": "Enter your JWT token"}
    }

    # Add custom tags for organization
    openapi_schema["tags"] = [
        {"name": "accounts", "description": "Chart of Accounts management"},
        {"name": "journal-entries", "description": "Journal Entry operations"},
        {"name": "ledgers", "description": "Ledger reports and queries"},
        {"name": "financial-statements", "description": "Financial statement generation"},
        {"name": "special-journals", "description": "Special journal operations"},
        {"name": "subsidiary-ledgers", "description": "Subsidiary ledger reports"},
        {"name": "petty-cash", "description": "Petty cash fund management"},
        {"name": "bank-reconciliation", "description": "Bank reconciliation operations"},
        {"name": "incomplete-records", "description": "Single-entry accounting"},
        {"name": "health", "description": "Service health checks"},
    ]

    app.openapi_schema = openapi_schema
    return app.openapi_schema


app = FastAPI(
    root_path="/api/v1",
    title="Vimbai Accounting Service",
    description="Manages Chart of Accounts, Journal Entries, Ledgers, Trial Balance, and Financial Statements.",
    version="0.1.0",
    docs_url="/docs",
    redoc_url="/redoc",
    openapi_url="/openapi.json",
)


# Bind the gateway-verified Book context to the request scope so every
# query executed on behalf of this request filters by Book.
@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


# Distributed tracing
try:
    from shared.tracing import get_tracer, setup_tracing

    TRACER = setup_tracing(service_name="accounting-service", instrument_app=app)
except ImportError:
    TRACER = None
    import logging

    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")

# Apply custom OpenAPI schema
app.openapi = custom_openapi


@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j"),
    )
    try:
        Neo4jConnector.get_driver()
        await Neo4jConnector.initialize_schema()
    except Exception as exc:  # pragma: no cover - startup resilience
        logger.warning("Neo4j not ready at startup; will retry on first query: %s", exc)


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
@app.post(
    "/accounts/",
    response_model=models.AccountInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.accounts"))],
)
async def create_new_account(
    account: models.AccountCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_account(db_session, user_id, account)


@app.get(
    "/accounts/{account_number}",
    response_model=models.AccountInDB,
    dependencies=[Depends(check_permission("accounting.read.accounts"))],
)
async def read_account_by_number(
    account_number: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    db_account = await crud.get_account(db_session, user_id, account_number)
    if db_account is None:
        raise NotFoundError(detail="Account not found.")
    return db_account


@app.get(
    "/accounts/",
    response_model=List[models.AccountInDB],
    dependencies=[Depends(check_permission("accounting.read.accounts"))],
)
async def read_all_accounts(user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)):
    return await crud.get_all_accounts(db_session, user_id)


@app.put(
    "/accounts/{account_number}",
    response_model=models.AccountInDB,
    dependencies=[Depends(check_permission("accounting.write.accounts"))],
)
async def update_existing_account(
    account_number: str,
    account: models.AccountUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    db_account = await crud.update_account(db_session, user_id, account_number, account)
    if db_account is None:
        raise NotFoundError(detail="Account not found.")
    return db_account


@app.delete(
    "/accounts/{account_number}",
    status_code=status.HTTP_204_NO_CONTENT,
    dependencies=[Depends(check_permission("accounting.delete.accounts"))],
)
async def delete_existing_account(
    account_number: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_account(db_session, user_id, account_number)
    if not success:
        raise NotFoundError(detail="Account not found or linked to existing entries.")
    return {"ok": True}


# --- NEW: Endpoint to get account activity for a period (for Budget Variance Report) ---
@app.get(
    "/accounts/{account_number}/period-activity",
    response_model=Dict[str, Decimal],
    dependencies=[Depends(check_permission("accounting.read.accounts"))],
)
async def get_account_activity_for_period(
    account_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: datetime = Query(..., description="Start date for the period (ISO format)."),
    end_date: datetime = Query(..., description="End date for the period (ISO format)."),
):
    total_debits, total_credits = await crud.get_account_period_activity(
        db_session, user_id, account_number, start_date, end_date
    )
    return {"total_debits": total_debits, "total_credits": total_credits}


@app.get("/", tags=["Health"])
async def root():
    """Root health probe."""
    return {"status": "ok", "service": "accounting-service"}


# --- Journal Entry Endpoints ---
@app.post(
    "/journal-entries/",
    response_model=models.JournalEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.journal_entries"))],
)
async def create_new_journal_entry(
    journal_entry: models.JournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),  # Pass JWT for internal calls
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_journal_entry(db_session, user_id, journal_entry, jwt_token)


@app.get(
    "/journal-entries/{entry_id}",
    response_model=models.JournalEntryInDB,
    dependencies=[Depends(check_permission("accounting.read.journal_entries"))],
)
async def read_journal_entry_by_id(
    entry_id: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    db_entry = await crud.get_journal_entry(db_session, user_id, entry_id)
    if db_entry is None:
        raise NotFoundError(detail="Journal entry not found.")
    return db_entry


@app.get(
    "/journal-entries/",
    response_model=List[models.JournalEntryInDB],
    dependencies=[Depends(check_permission("accounting.read.journal_entries"))],
)
async def read_all_journal_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None, description="Start date for filtering journal entries (ISO format)."),
    end_date: Optional[datetime] = Query(None, description="End date for filtering journal entries (ISO format)."),
):
    return await crud.get_all_journal_entries(db_session, user_id, start_date, end_date)


@app.put(
    "/journal-entries/{entry_id}",
    response_model=models.JournalEntryInDB,
    dependencies=[Depends(check_permission("accounting.write.journal_entries"))],
)
async def update_existing_journal_entry(
    entry_id: str,
    journal_entry: models.JournalEntryUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    db_entry = await crud.update_journal_entry(db_session, user_id, entry_id, journal_entry)
    if db_entry is None:
        raise NotFoundError(detail="Journal entry not found.")
    return db_entry


@app.delete(
    "/journal-entries/{entry_id}",
    status_code=status.HTTP_204_NO_CONTENT,
    dependencies=[Depends(check_permission("accounting.delete.journal_entries"))],
)
async def delete_existing_journal_entry(
    entry_id: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_journal_entry(db_session, user_id, entry_id)
    if not success:
        raise NotFoundError(detail="Journal entry not found.")
    return {"ok": True}


# --- Vendor Bill Endpoints (NEW) ---
@app.post(
    "/vendor-bills/",
    response_model=models.VendorBillInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.create.vendor_bill"))],
)
async def create_vendor_bill(
    vendor_bill: models.VendorBillCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_vendor_bill(db_session, user_id, vendor_bill, jwt_token)


# --- Ledger Endpoints ---
@app.get(
    "/ledgers/{account_number}",
    response_model=models.LedgerReport,
    dependencies=[Depends(check_permission("accounting.read.ledgers"))],
)
async def get_account_ledger(
    account_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None, description="Start date for ledger entries (ISO format)."),
    end_date: Optional[datetime] = Query(None, description="End date for ledger entries (ISO format)."),
):
    return await crud.get_ledger_report(db_session, user_id, account_number, start_date, end_date)


# --- Trial Balance Endpoints ---
@app.get(
    "/trial-balance/",
    response_model=models.TrialBalanceReport,
    dependencies=[Depends(check_permission("accounting.read.trial_balance"))],
)
async def get_current_trial_balance(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None, description="Date to generate trial balance as of (ISO format)."),
):
    return await crud.get_trial_balance_report(db_session, user_id, as_of_date)


# --- Income Statement Endpoints ---
@app.get(
    "/income-statement/",
    response_model=models.IncomeStatement,
    dependencies=[Depends(check_permission("accounting.read.income_statement"))],
)
async def get_income_statement_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: datetime = Query(..., description="Start date for the income statement period (ISO format)."),
    end_date: datetime = Query(..., description="End date for the income statement period (ISO format)."),
):
    return await crud.get_income_statement(db_session, user_id, start_date, end_date)


# --- Balance Sheet Endpoints ---
@app.get(
    "/balance-sheet/",
    response_model=models.BalanceSheet,
    dependencies=[Depends(check_permission("accounting.read.balance_sheet"))],
)
async def get_balance_sheet_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: datetime = Query(..., description="Date to generate the balance sheet as of (ISO format)."),
):
    return await crud.get_balance_sheet(db_session, user_id, as_of_date)


# --- Sales Journal Endpoints ---
@app.post(
    "/sales-journal/",
    response_model=models.SalesJournalEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.sales_journal"))],
)
async def create_sales_journal_entry(
    entry: models.SalesJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_sales_journal_entry(db_session, user_id, entry, jwt_token)


@app.get(
    "/sales-journal/",
    response_model=List[models.SalesJournalEntryInDB],
    dependencies=[Depends(check_permission("accounting.read.sales_journal"))],
)
async def read_sales_journal_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None, description="Start date for filtering"),
    end_date: Optional[datetime] = Query(None, description="End date for filtering"),
    customer_id: Optional[str] = Query(None, description="Filter by customer ID"),
    status: Optional[str] = Query(None, description="Filter by status"),
):
    return await crud.get_sales_journal_entries(db_session, user_id, start_date, end_date, customer_id, status)


@app.get(
    "/sales-journal/{entry_id}",
    response_model=models.SalesJournalEntryInDB,
    dependencies=[Depends(check_permission("accounting.read.sales_journal"))],
)
async def read_sales_journal_entry(
    entry_id: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_sales_journal_entry(db_session, user_id, entry_id)


# --- Purchases Journal Endpoints ---
@app.post(
    "/purchases-journal/",
    response_model=models.PurchasesJournalEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.purchases_journal"))],
)
async def create_purchases_journal_entry(
    entry: models.PurchasesJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_purchases_journal_entry(db_session, user_id, entry, jwt_token)


@app.get(
    "/purchases-journal/",
    response_model=List[models.PurchasesJournalEntryInDB],
    dependencies=[Depends(check_permission("accounting.read.purchases_journal"))],
)
async def read_purchases_journal_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None),
    vendor_id: Optional[str] = Query(None),
    status: Optional[str] = Query(None),
):
    return await crud.get_purchases_journal_entries(db_session, user_id, start_date, end_date, vendor_id, status)


# --- Cash Receipts Journal Endpoints ---
@app.post(
    "/cash-receipts-journal/",
    response_model=models.CashReceiptsJournalEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.cash_receipts"))],
)
async def create_cash_receipts_entry(
    entry: models.CashReceiptsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_cash_receipts_entry(db_session, user_id, entry, jwt_token)


@app.get(
    "/cash-receipts-journal/",
    response_model=List[models.CashReceiptsJournalEntryInDB],
    dependencies=[Depends(check_permission("accounting.read.cash_receipts"))],
)
async def read_cash_receipts_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None),
):
    return await crud.get_cash_receipts_entries(db_session, user_id, start_date, end_date)


# --- Cash Disbursements Journal Endpoints ---
@app.post(
    "/cash-disbursements-journal/",
    response_model=models.CashDisbursementsJournalEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.cash_disbursements"))],
)
async def create_cash_disbursements_entry(
    entry: models.CashDisbursementsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_cash_disbursements_entry(db_session, user_id, entry, jwt_token)


@app.get(
    "/cash-disbursements-journal/",
    response_model=List[models.CashDisbursementsJournalEntryInDB],
    dependencies=[Depends(check_permission("accounting.read.cash_disbursements"))],
)
async def read_cash_disbursements_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None),
):
    return await crud.get_cash_disbursements_entries(db_session, user_id, start_date, end_date)


# --- Sales Returns Journal Endpoints ---
@app.post(
    "/sales-returns-journal/",
    response_model=models.SalesReturnsJournalEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.sales_returns"))],
)
async def create_sales_return_entry(
    entry: models.SalesReturnsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_sales_return_entry(db_session, user_id, entry, jwt_token)


# --- Purchases Returns Journal Endpoints ---
@app.post(
    "/purchases-returns-journal/",
    response_model=models.PurchasesReturnsJournalEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.purchases_returns"))],
)
async def create_purchases_return_entry(
    entry: models.PurchasesReturnsJournalEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_purchases_return_entry(db_session, user_id, entry, jwt_token)


# --- Subsidiary Ledgers Endpoints ---
@app.get(
    "/subsidiary-ledgers/accounts-receivable/",
    response_model=models.AccountsReceivableLedgerReport,
    dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))],
)
async def get_ar_ledger_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None, description="Date for AR aging"),
    customer_id: Optional[str] = Query(None, description="Filter by customer"),
):
    return await crud.get_ar_ledger_report(db_session, user_id, as_of_date, customer_id)


@app.get(
    "/subsidiary-ledgers/accounts-payable/",
    response_model=models.AccountsPayableLedgerReport,
    dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))],
)
async def get_ap_ledger_report(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None, description="Date for AP aging"),
    vendor_id: Optional[str] = Query(None, description="Filter by vendor"),
):
    return await crud.get_ap_ledger_report(db_session, user_id, as_of_date, vendor_id)


@app.get(
    "/subsidiary-ledgers/fixed-assets/",
    response_model=models.FixedAssetsLedgerReport,
    dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))],
)
async def get_fixed_assets_ledger(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None),
    status: Optional[str] = Query(None, description="Filter by asset status"),
):
    return await crud.get_fixed_assets_ledger(db_session, user_id, as_of_date, status)


@app.get(
    "/subsidiary-ledgers/inventory/",
    response_model=models.InventoryLedgerReport,
    dependencies=[Depends(check_permission("accounting.read.subsidiary_ledgers"))],
)
async def get_inventory_ledger(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    as_of_date: Optional[datetime] = Query(None),
    category: Optional[str] = Query(None, description="Filter by category"),
):
    return await crud.get_inventory_ledger(db_session, user_id, as_of_date, category)


# --- Petty Cash Endpoints ---
@app.post(
    "/petty-cash-funds/",
    response_model=models.PettyCashFundInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.petty_cash"))],
)
async def create_petty_cash_fund(
    fund: models.PettyCashFundCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_petty_cash_fund(db_session, user_id, fund)


@app.get(
    "/petty-cash-funds/",
    response_model=List[models.PettyCashFundInDB],
    dependencies=[Depends(check_permission("accounting.read.petty_cash"))],
)
async def read_petty_cash_funds(
    user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_petty_cash_funds(db_session, user_id)


@app.post(
    "/petty-cash-entries/",
    response_model=models.PettyCashEntryInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.petty_cash"))],
)
async def create_petty_cash_entry(
    entry: models.PettyCashEntryCreate,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_petty_cash_entry(db_session, user_id, entry, jwt_token)


@app.get(
    "/petty-cash-entries/",
    response_model=List[models.PettyCashEntryInDB],
    dependencies=[Depends(check_permission("accounting.read.petty_cash"))],
)
async def read_petty_cash_entries(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    fund_id: Optional[str] = Query(None, description="Filter by fund ID"),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None),
):
    return await crud.get_petty_cash_entries(db_session, user_id, fund_id, start_date, end_date)


# --- Bank Reconciliation Endpoints ---
@app.post(
    "/bank-reconciliation/",
    response_model=models.BankReconciliationStatement,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.bank_reconciliation"))],
)
async def create_bank_reconciliation(
    bank_account: str,
    statement_date: datetime,
    statement_balance: Decimal,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.create_bank_reconciliation(
        db_session, user_id, bank_account, statement_date, statement_balance, jwt_token
    )


@app.get(
    "/bank-reconciliation/",
    response_model=List[models.BankReconciliationStatement],
    dependencies=[Depends(check_permission("accounting.read.bank_reconciliation"))],
)
async def get_bank_reconciliations(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    bank_account: Optional[str] = Query(None),
    start_date: Optional[datetime] = Query(None),
    end_date: Optional[datetime] = Query(None),
):
    return await crud.get_bank_reconciliations(db_session, user_id, bank_account, start_date, end_date)


@app.get(
    "/bank-reconciliation/latest/{bank_account}",
    response_model=models.BankReconciliationStatement,
    dependencies=[Depends(check_permission("accounting.read.bank_reconciliation"))],
)
async def get_latest_reconciliation(
    bank_account: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_latest_bank_reconciliation(db_session, user_id, bank_account)


# =============================================================================
# INCOMPLETE RECORDS / SINGLE ENTRY SYSTEM ENDPOINTS
# =============================================================================

# --- Statement of Affairs Endpoints ---


@app.post(
    "/statements-of-affairs/",
    response_model=models.StatementOfAffairsInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.incomplete_records"))],
)
async def create_statement_of_affairs(
    as_of_date: datetime,
    assets: List[models.StatementOfAffairsAssetBase],
    liabilities: List[models.StatementOfAffairsLiabilityBase],
    prepared_by: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create Statement of Affairs - shows assets, liabilities and capital at a point in time (similar to Balance Sheet but for single entry businesses)"""
    return await crud.create_statement_of_affairs(db_session, user_id, as_of_date, assets, liabilities, prepared_by)


@app.get(
    "/statements-of-affairs/",
    response_model=List[models.StatementOfAffairsInDB],
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_all_statements_of_affairs(
    user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    """Get all Statements of Affairs"""
    return await crud.get_all_statements_of_affairs(db_session, user_id)


@app.get(
    "/statements-of-affairs/{as_of_date}",
    response_model=models.StatementOfAffairsInDB,
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_statement_of_affairs_by_date(
    as_of_date: datetime, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    """Get Statement of Affairs as of a specific date"""
    return await crud.get_statement_of_affairs(db_session, user_id, as_of_date)


# --- Capital Calculation Endpoints ---


@app.post(
    "/capital-calculations/",
    response_model=models.CapitalCalculationInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.incomplete_records"))],
)
async def create_capital_calculation(
    calc: models.CapitalCalculationInDB,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create Capital Calculation - tracks capital changes over a period"""
    return await crud.create_capital_calculation(db_session, user_id, calc)


@app.get(
    "/capital-calculations/",
    response_model=List[models.CapitalCalculationInDB],
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_all_capital_calculations(
    user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    """Get all Capital Calculations"""
    return await crud.get_all_capital_calculations(db_session, user_id)


@app.get(
    "/capital-calculations/{calc_id}",
    response_model=models.CapitalCalculationInDB,
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_capital_calculation_by_id(
    calc_id: str, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    """Get Capital Calculation by ID"""
    return await crud.get_capital_calculation(db_session, user_id, calc_id)


# --- Control Account Endpoints (Debtors & Creditors) ---


@app.post(
    "/control-accounts/",
    response_model=models.ControlAccountInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.incomplete_records"))],
)
async def create_control_account(
    account: models.ControlAccountInDB,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create Control Account - tracks debtors or creditors balances"""
    return await crud.create_control_account(db_session, user_id, account)


@app.get(
    "/control-accounts/",
    response_model=List[models.ControlAccountInDB],
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_all_control_accounts(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
    account_type: Optional[str] = Query(None, description="Filter by type: 'debtors' or 'creditors'"),
):
    """Get all Control Accounts, optionally filtered by type"""
    return await crud.get_control_accounts(db_session, user_id, account_type)


# --- Receipts and Payments Account Endpoints ---


@app.post(
    "/receipts-payments/",
    response_model=models.ReceiptsPaymentsAccountInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.incomplete_records"))],
)
async def create_receipts_payments_account(
    rp: models.ReceiptsPaymentsAccountInDB,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create Receipts and Payments Account - cash book summary for single entry businesses"""
    return await crud.create_receipts_payments_account(db_session, user_id, rp)


@app.get(
    "/receipts-payments/",
    response_model=List[models.ReceiptsPaymentsAccountInDB],
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_all_receipts_payments_accounts(
    user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    """Get all Receipts and Payments Accounts"""
    return await crud.get_receipts_payments_accounts(db_session, user_id)


# --- Single Entry Conversion Endpoints ---


@app.post(
    "/single-entry-conversions/",
    response_model=models.SingleEntryConversionInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.incomplete_records"))],
)
async def create_single_entry_conversion(
    conversion: models.SingleEntryConversionInDB,
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create Single Entry Conversion - converts single entry records to double entry system"""
    return await crud.create_single_entry_conversion(db_session, user_id, conversion, jwt_token)


@app.get(
    "/single-entry-conversions/",
    response_model=List[models.SingleEntryConversionInDB],
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_all_single_entry_conversions(
    user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    """Get all Single Entry Conversions"""
    return await crud.get_single_entry_conversions(db_session, user_id)


# --- Profit Estimation Endpoints ---


@app.post(
    "/profit-estimations/",
    response_model=models.ProfitEstimationInDB,
    status_code=status.HTTP_201_CREATED,
    dependencies=[Depends(check_permission("accounting.write.incomplete_records"))],
)
async def create_profit_estimation(
    estimation: models.ProfitEstimationInDB,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create Profit Estimation - calculates profit/loss from capital changes (alternative method for single entry)"""
    return await crud.create_profit_estimation(db_session, user_id, estimation)


@app.get(
    "/profit-estimations/",
    response_model=List[models.ProfitEstimationInDB],
    dependencies=[Depends(check_permission("accounting.read.incomplete_records"))],
)
async def get_all_profit_estimations(
    user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)
):
    """Get all Profit Estimations"""
    return await crud.get_profit_estimations(db_session, user_id)
