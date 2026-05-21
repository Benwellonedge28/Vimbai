from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from banking_integration_service.models import (
    BankCreate, BankUpdate, BankInDB,
    BankAccountCreate, BankAccountUpdate, BankAccountInDB,
    TransactionCreate, TransactionUpdate, TransactionInDB,
    JournalEntryCreate, CreateJournalEntryResponse, JournalLineBase,
    TransactionForFraudCheck, FraudDetectionResult # NEW
)
from datetime import datetime, timedelta
import uuid
from decimal import Decimal
import httpx
import os
from banking_integration_service.exceptions import ValidationError, NotFoundError # Corrected import

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# --- Bank CRUD (unchanged) ---
# ...

# --- Bank Account CRUD (unchanged) ---
# ...

# --- Transaction CRUD ---
async def create_transaction(session: AsyncSession, user_id: str, transaction_data: TransactionCreate, jwt_token: str) -> TransactionInDB:
    transaction_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Create Transaction node and link to BankAccount
    transaction_query = """
    MATCH (ba:BankAccount {id: $bank_account_id, user_id: $user_id})
    CREATE (t:Transaction {
        id: $id,
        transaction_id: $transaction_id,
        description: $description,
        amount: toFloat($amount),
        currency: $currency,
        transaction_date: datetime($transaction_date),
        post_date: datetime($post_date),
        category: $category,
        accounting_account_number: $accounting_account_number,
        journal_entry_id: $journal_entry_id,
        fraud_flag: $fraud_flag, # NEW
        fraud_score: $fraud_score, # NEW
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (ba)-[:HAS_TRANSACTION]->(t)
    RETURN t
    """
    transaction_params = transaction_data.model_dump()
    transaction_params["id"] = transaction_neo4j_id
    transaction_params["user_id"] = user_id # Used for matching bank_account
    transaction_params["amount"] = float(transaction_params["amount"])
    transaction_params["transaction_date"] = transaction_params["transaction_date"].isoformat()
    if transaction_params["post_date"]:
        transaction_params["post_date"] = transaction_params["post_date"].isoformat()
    transaction_params["created_at"] = created_at.isoformat()
    transaction_params["updated_at"] = updated_at.isoformat()

    # --- NEW: Call Fraud Detection Service ---
    fraud_result = await _analyze_transaction_for_fraud(session, user_id, transaction_data, jwt_token)
    transaction_params["fraud_flag"] = fraud_result.fraud_flag
    transaction_params["fraud_score"] = fraud_result.fraud_score
    # ----------------------------------------

    result = await session.run(transaction_query, transaction_params)
    record = await result.single()
    transaction_node = record["t"]
    
    # Return full TransactionInDB object, including newly set fraud fields
    return TransactionInDB(
        id=transaction_node["id"],
        user_id=user_id,
        bank_account_id=transaction_node["bank_account_id"],
        transaction_id=transaction_node["transaction_id"],
        description=transaction_node["description"],
        amount=Decimal(str(transaction_node["amount"])),
        currency=transaction_node["currency"],
        transaction_date=datetime.fromisoformat(transaction_node["transaction_date"].iso_format()),
        post_date=datetime.fromisoformat(transaction_node["post_date"].iso_format()) if transaction_node["post_date"] else None,
        category=transaction_node["category"],
        accounting_account_number=transaction_node["accounting_account_number"],
        journal_entry_id=transaction_node["journal_entry_id"],
        fraud_flag=transaction_node["fraud_flag"],
        fraud_score=transaction_node["fraud_score"],
        created_at=datetime.fromisoformat(transaction_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(transaction_node["updated_at"].iso_format()),
    )


async def get_transaction(session: AsyncSession, transaction_id: str, user_id: str) -> Optional[TransactionInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BANK_ACCOUNT]->(ba:BankAccount)-[:HAS_TRANSACTION]->(t:Transaction {id: $transaction_id})
    RETURN t, ba.id AS bank_account_id
    """
    result = await session.run(query, transaction_id=transaction_id, user_id=user_id)
    record = await result.single()

    if record:
        transaction_node = record["t"]
        return TransactionInDB(
            id=transaction_node["id"],
            user_id=user_id,
            bank_account_id=record["bank_account_id"],
            transaction_id=transaction_node["transaction_id"],
            description=transaction_node["description"],
            amount=Decimal(str(transaction_node["amount"])),
            currency=transaction_node["currency"],
            transaction_date=datetime.fromisoformat(transaction_node["transaction_date"].iso_format()),
            post_date=datetime.fromisoformat(transaction_node["post_date"].iso_format()) if transaction_node["post_date"] else None,
            category=transaction_node["category"],
            accounting_account_number=transaction_node["accounting_account_number"],
            journal_entry_id=transaction_node["journal_entry_id"],
            fraud_flag=transaction_node["fraud_flag"],
            fraud_score=transaction_node["fraud_score"],
            created_at=datetime.fromisoformat(transaction_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(transaction_node["updated_at"].iso_format()),
        )
    return None

async def get_all_transactions(session: AsyncSession, user_id: str) -> List[TransactionInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BANK_ACCOUNT]->(ba:BankAccount)-[:HAS_TRANSACTION]->(t:Transaction)
    RETURN t, ba.id AS bank_account_id
    ORDER BY t.transaction_date DESC
    """
    result = await session.run(query, user_id=user_id)
    transactions = []
    async for record in result:
        transaction_node = record["t"]
        transactions.append(TransactionInDB(
            id=transaction_node["id"],
            user_id=user_id,
            bank_account_id=record["bank_account_id"],
            transaction_id=transaction_node["transaction_id"],
            description=transaction_node["description"],
            amount=Decimal(str(transaction_node["amount"])),
            currency=transaction_node["currency"],
            transaction_date=datetime.fromisoformat(transaction_node["transaction_date"].iso_format()),
            post_date=datetime.fromisoformat(transaction_node["post_date"].iso_format()) if transaction_node["post_date"] else None,
            category=transaction_node["category"],
            accounting_account_number=transaction_node["accounting_account_number"],
            journal_entry_id=transaction_node["journal_entry_id"],
            fraud_flag=transaction_node["fraud_flag"],
            fraud_score=transaction_node["fraud_score"],
            created_at=datetime.fromisoformat(transaction_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(transaction_node["updated_at"].iso_format()),
        ))
    return transactions

async def update_transaction(session: AsyncSession, transaction_id: str, user_id: str, transaction_data: TransactionUpdate) -> Optional[TransactionInDB]:
    update_fields = transaction_data.model_dump(exclude_unset=True)
    if not update_fields: # If no fields to update, return current transaction
        return await get_transaction(session, transaction_id, user_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "amount" in update_fields:
        update_fields["amount"] = float(update_fields["amount"])
    if "transaction_date" in update_fields and update_fields["transaction_date"]:
        update_fields["transaction_date"] = update_fields["transaction_date"].isoformat()
    if "post_date" in update_fields and update_fields["post_date"]:
        update_fields["post_date"] = update_fields["post_date"].isoformat()

    set_clauses = [f"t.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_ACCOUNT]->(ba:BankAccount)-[:HAS_TRANSACTION]->(t:Transaction {{id: $transaction_id}})
    SET {set_query_part}
    RETURN t
    """
    
    params = {"transaction_id": transaction_id, "user_id": user_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_transaction(session, transaction_id, user_id)
    return None

async def delete_transaction(session: AsyncSession, transaction_id: str, user_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BANK_ACCOUNT]->(ba:BankAccount)-[:HAS_TRANSACTION]->(t:Transaction {id: $transaction_id})
    DETACH DELETE t
    """
    result = await session.run(query, transaction_id=transaction_id, user_id=user_id)
    return result.consume().counters.nodes_deleted > 0

async def analyze_transaction_and_create_journal_entry(
    session: AsyncSession, 
    transaction_id: str, 
    user_id: str, 
    debit_account_number: str, 
    credit_account_number: str, 
    jwt_token: str
) -> CreateJournalEntryResponse:
    
    transaction = await get_transaction(session, transaction_id, user_id)
    if not transaction:
        raise NotFoundError(detail="Transaction not found.")

    if transaction.journal_entry_id:
        raise ConflictError(detail="Journal entry already exists for this transaction.", code="JOURNAL_ENTRY_EXISTS_FOR_TRANSACTION")
    
    # Determine debit/credit for JE
    je_amount = abs(transaction.amount) # Always positive for JE lines
    
    je_lines = [
        JournalLineBase(account_number=debit_account_number, debit=je_amount, credit=Decimal('0.00'), description=transaction.description),
        JournalLineBase(account_number=credit_account_number, debit=Decimal('0.00'), credit=je_amount, description=transaction.description)
    ]
    
    journal_entry_create = JournalEntryCreate(
        entry_date=transaction.transaction_date,
        description=f"Automated JE for transaction {transaction.transaction_id}: {transaction.description}",
        reference_number=transaction.transaction_id,
        source_module="Banking",
        lines=je_lines
    )

    # Send to Accounting Service via API Gateway
    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    async with httpx.AsyncClient() as client:
        try:
            response = await client.post(
                f"{API_GATEWAY_URL}/journal-entries/",
                headers=headers,
                json=journal_entry_create.model_dump(by_alias=True)
            )
            response.raise_for_status() # Raise HTTPStatusError for bad responses (4xx or 5xx)
            je_response_data = response.json()
            journal_entry_id = je_response_data.get("id")

            # Update transaction with journal_entry_id
            await update_transaction(session, transaction_id, user_id, TransactionUpdate(journal_entry_id=journal_entry_id))

            return CreateJournalEntryResponse(
                status="success",
                message=f"Journal entry created for transaction {transaction.transaction_id}.",
                journal_entry_id=journal_entry_id
            )
        except httpx.HTTPStatusError as e:
            # Propagate specific error details from Accounting Service if available
            error_detail = e.response.json().get("detail", e.response.text)
            error_code = e.response.json().get("code", "UPSTREAM_JE_ERROR")
            raise ValidationError(detail=f"Failed to create journal entry in Accounting Service: {error_detail}", code=error_code)
        except httpx.RequestError as e:
            raise ValidationError(detail=f"Network error communicating with Accounting Service: {e}", code="UPSTREAM_JE_NETWORK_ERROR")

async def _analyze_transaction_for_fraud(session: AsyncSession, user_id: str, transaction_data: TransactionCreate, jwt_token: str) -> FraudDetectionResult:
    """Internal helper to send transaction to Fraud Detection Service."""
    
    # Placeholder: fetch some historical data for transaction context
    # In a real system, this would query the DB for user's past transactions
    # For now, we'll use dummy values
    previous_transactions_count_24h = 0 # Dummy
    avg_daily_transaction_amount_7d = Decimal('0.00') # Dummy

    # Construct TransactionForFraudCheck model
    # Note: recipient_account_id and sender_account_id are often inferred or come from external bank data.
    # For now, we'll use dummy values or transaction_data.bank_account_id
    transaction_for_fraud = TransactionForFraudCheck(
        transaction_id=transaction_data.transaction_id,
        amount=abs(transaction_data.amount), # Fraud detection often focuses on absolute amount
        currency=transaction_data.currency,
        sender_account_id=transaction_data.bank_account_id, # Assuming bank_account is sender
        recipient_account_id="EXTERNAL_PARTY_ID", # Placeholder
        transaction_type="payment" if transaction_data.amount < 0 else "receipt", # Infer type
        timestamp=transaction_data.transaction_date,
        previous_transactions_count_24h=previous_transactions_count_24h,
        avg_daily_transaction_amount_7d=avg_daily_transaction_amount_7d
    )

    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    async with httpx.AsyncClient() as client:
        try:
            response = await client.post(
                f"{API_GATEWAY_URL}/fraud-detection/analyze-transaction/",
                headers=headers,
                json=transaction_for_fraud.model_dump(by_alias=True)
            )
            response.raise_for_status()
            return FraudDetectionResult(**response.json())
        except httpx.HTTPStatusError as e:
            error_detail = e.response.json().get("detail", e.response.text)
            error_code = e.response.json().get("code", "UPSTREAM_FRAUD_DETECTION_ERROR")
            print(f"Fraud Detection Service error: {error_detail}")
            # If fraud detection fails, we still create the transaction, but log the error
            return FraudDetectionResult(
                transaction_id=transaction_data.transaction_id,
                fraud_score=0.0,
                fraud_flag="safe", # Default to safe if service fails
                reason=f"Fraud detection service failed: {error_detail}",
                model_version="N/A_service_unavailable"
            )
        except httpx.RequestError as e:
            print(f"Network error communicating with Fraud Detection Service: {e}")
            return FraudDetectionResult(
                transaction_id=transaction_data.transaction_id,
                fraud_score=0.0,
                fraud_flag="safe", # Default to safe if service unavailable
                reason=f"Network error connecting to Fraud Detection Service: {e}",
                model_version="N/A_network_error"
            )
