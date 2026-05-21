from fastapi import FastAPI, Depends, HTTPException, status, Request
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from banking_integration_service import models, crud
from banking_integration_service.database import init_db_schema, Neo4jConnector
from banking_integration_service.dependencies import get_db_session, get_user_id
from banking_integration_service.utils.auth import check_permission
from banking_integration_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
from banking_integration_service.clients.plaid_client import PlaidClient, PlaidClientException # NEW
import os
from dotenv import load_dotenv
from datetime import date, timedelta # NEW: timedelta

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Banking Integration Service",
    description="Manages bank connections, synchronizes transactions, and facilitates reconciliation.",
    version="0.1.0",
)

plaid_client = PlaidClient() # Initialize Plaid client

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

# --- Global Exception Handlers ---
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

@app.exception_handler(PlaidClientException)
async def plaid_exception_handler(request, exc: PlaidClientException):
    return JSONResponse(
        status_code=exc.status_code or status.HTTP_500_INTERNAL_SERVER_ERROR,
        content={"message": exc.args[0], "plaid_error_code": exc.error_code}
    )

# --- Bank Connection Endpoints ---
@app.post("/connections/", response_model=models.BankConnectionInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.connections"))])
async def create_bank_connection(
    connection: models.BankConnectionCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
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

# --- Plaid Integration Endpoints (NEW) ---
@app.post("/plaid/link/token", status_code=status.HTTP_200_OK,
              dependencies=[Depends(check_permission("banking.write.connections"))])
async def create_plaid_link_token(
    request: Request,
    user_id: str = Depends(get_user_id)
):
    try:
        # Client name could come from user settings or a predefined app name
        client_name = os.getenv("PLAID_CLIENT_APP_NAME", "FinAcc")
        link_token_response = await plaid_client.create_link_token(user_id=user_id, client_name=client_name)
        return {"link_token": link_token_response["link_token"]}
    except PlaidClientException as e:
        raise HTTPException(
            status_code=e.status_code,
            detail={"message": e.args[0], "plaid_error_code": e.error_code}
        )

@app.post("/plaid/public-token/exchange", response_model=models.BankConnectionInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("banking.write.connections"))])
async def exchange_plaid_public_token(
    public_token_data: Dict[str, str], # Expects {"public_token": "..."}
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    public_token = public_token_data.get("public_token")
    if not public_token:
        raise ValidationError(detail="Public token is required.")

    try:
        exchange_response = await plaid_client.exchange_public_token(public_token)
        access_token = exchange_response["access_token"]
        item_id = exchange_response["item_id"]

        # Check if a connection with this item_id already exists for the user
        existing_connections = await crud.get_all_bank_connections(db_session, user_id)
        for conn in existing_connections:
            if conn.external_id == item_id:
                raise ConflictError(detail="This Plaid institution is already connected.")

        # Create new BankConnection in our DB
        new_connection_data = models.BankConnectionCreate(
            user_id=user_id,
            provider="Plaid",
            access_token=access_token, # Store encrypted token in real app
            external_id=item_id,
            status="active"
        )
        bank_connection = await crud.create_bank_connection(db_session, new_connection_data)

        # Optionally, fetch accounts immediately and store them
        accounts_response = await plaid_client.get_accounts(access_token)
        for account_data in accounts_response["accounts"]:
            # Check if this account already exists for the connection
            existing_accounts = await crud.get_bank_accounts_for_connection(db_session, bank_connection.id)
            if not any(acc.account_id == account_data["account_id"] for acc in existing_accounts):
                account_model = models.BankAccountCreate(
                    account_id=account_data["account_id"],
                    connection_id=bank_connection.id,
                    name=account_data["name"],
                    mask=account_data["mask"],
                    type=account_data["type"],
                    subtype=account_data["subtype"],
                    currency=account_data["balances"]["iso_currency_code"],
                    current_balance=account_data["balances"]["current"],
                    available_balance=account_data["balances"]["available"],
                    status="active"
                )
                await crud.create_bank_account(db_session, bank_connection.id, account_model)

        return bank_connection

    except PlaidClientException as e:
        raise HTTPException(
            status_code=e.status_code,
            detail={"message": e.args[0], "plaid_error_code": e.error_code}
        )

@app.post("/connections/{connection_id}/sync-transactions", status_code=status.HTTP_200_OK,
              dependencies=[Depends(check_permission("banking.read.transactions"))])
async def sync_transactions_for_connection(
    connection_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    bank_connection = await crud.get_bank_connection(db_session, user_id, connection_id)
    if not bank_connection:
        raise NotFoundError(detail="Bank Connection not found or not owned by user.")

    try:
        # Determine date range for transactions to fetch
        # For simplicity, fetch last 30 days, or from last_synced_at if available
        end_date = date.today().isoformat()
        start_date = (date.today() - timedelta(days=30)).isoformat()
        if bank_connection.last_synced_at:
            # Fetch from the day after last sync
            start_date = (bank_connection.last_synced_at.date() + timedelta(days=1)).isoformat()
        
        transactions_response = await plaid_client.get_transactions(
            access_token=bank_connection.access_token,
            start_date=start_date,
            end_date=end_date,
            options={"count": 500} # Max transactions to fetch
        )

        synced_count = 0
        for tx_data in transactions_response["transactions"]:
            # Check if transaction already exists by provider transaction_id
            # This requires a new CRUD function: get_bank_transaction_by_provider_id
            existing_tx = await crud.get_bank_transaction_by_provider_id(db_session, tx_data["transaction_id"]) # Need to implement this in crud
            if not existing_tx:
                # Find the local BankAccount for this Plaid account_id
                accounts = await crud.get_bank_accounts_for_connection(db_session, connection_id)
                target_account = next((acc for acc in accounts if acc.account_id == tx_data["account_id"]), None)

                if target_account:
                    # Create BankTransaction in our DB
                    transaction_model = models.BankTransactionCreate(
                        transaction_id=tx_data["transaction_id"],
                        account_id=target_account.id, # Our internal account ID
                        description=tx_data["name"],
                        amount=tx_data["amount"],
                        date=date.fromisoformat(tx_data["date"]),
                        posted_date=date.fromisoformat(tx_data["date"]), # Plaid 'date' is usually posted date
                        category=tx_data["personal_finance_category"]["primary"] if tx_data.get("personal_finance_category") else None,
                        type=tx_data["transaction_type"],
                        status=tx_data["pending_transaction_id"] and "pending" or "posted", # Plaid has pending transactions
                        metadata=tx_data # Store full Plaid data in metadata for now
                    )
                    await crud.create_bank_transaction(db_session, target_account.id, transaction_model)
                    synced_count += 1
                else:
                    print(f"Warning: Plaid account ID {tx_data['account_id']} not found in FinAcc for connection {connection_id}. Transaction skipped.")

        # Update last_synced_at for the connection
        await crud.update_bank_connection(db_session, user_id, connection_id, models.BankConnectionUpdate(last_synced_at=datetime.utcnow()))

        return {"message": f"Successfully synced {synced_count} new transactions for connection {connection_id}."}

    except PlaidClientException as e:
        raise HTTPException(
            status_code=e.status_code,
            detail={"message": e.args[0], "plaid_error_code": e.error_code}
        )

@app.post("/plaid/webhook", status_code=status.HTTP_200_OK)
async def plaid_webhook_receiver(
    webhook_data: Dict[str, Any],
    db_session: AsyncSession = Depends(get_db_session)
):
    webhook_type = webhook_data.get("webhook_type")
    webhook_code = webhook_data.get("webhook_code")
    item_id = webhook_data.get("item_id")

    print(f"Received Plaid webhook: Type={webhook_type}, Code={webhook_code}, Item_ID={item_id}")

    if webhook_type == "TRANSACTIONS" and webhook_code == "TRANSACTIONS_REMOVED":
        print(f"Transactions removed for Item {item_id}. Need to update FinAcc transactions.")
    elif webhook_type == "TRANSACTIONS" and webhook_code in ["DEFAULT_UPDATE", "HISTORICAL_UPDATE", "INITIAL_UPDATE"]:
        print(f"New transactions available for Item {item_id}. Triggering transaction sync.")
        # In a real system, you would enqueue a background job to sync transactions for this item_id
        pass

    return {"status": "ok", "message": "Webhook received and acknowledged."}


# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Banking Integration Service is running!"}
