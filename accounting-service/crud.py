from neo4j import AsyncSession
from typing import Optional, List, Dict, Any, Tuple
from accounting_service.models import (
    AccountCreate, AccountUpdate, AccountInDB,
    JournalEntryCreate, JournalEntryUpdate, JournalEntryInDB, JournalLineBase,
    LedgerReport, LedgerEntry, TrialBalanceReport, TrialBalanceAccount,
    IncomeStatement, BalanceSheet, FinancialStatementLine,
    TransactionForFraudCheck, FraudDetectionResult # NEW
)
from datetime import datetime, timedelta
import uuid
from decimal import Decimal
import httpx
import os
from accounting_service.exceptions import ValidationError, NotFoundError, ConflictError

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# --- Account CRUD (unchanged) ---
# ...

# --- Journal Entry CRUD ---
async def create_journal_entry(session: AsyncSession, user_id: str, journal_entry_data: JournalEntryCreate, jwt_token: str) -> JournalEntryInDB:
    entry_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Verify all account numbers exist and belong to the user
    for line in journal_entry_data.lines:
        account = await get_account(session, user_id, line.account_number)
        if not account:
            raise ValidationError(detail=f"Account number {line.account_number} not found or does not belong to user.", code="ACCOUNT_NOT_FOUND_FOR_JE")
        # Optional: Validate normal balance consistency (e.g., debiting a credit-normal revenue account)
        # For now, we assume this is handled by the caller or downstream review.

    # Check for existing reference number for entries from specific source modules
    if journal_entry_data.reference_number and journal_entry_data.source_module in ["Banking", "Invoicing", "Multimodal"]:
        existing_je = await get_journal_entry_by_reference(session, user_id, journal_entry_data.reference_number, journal_entry_data.source_module)
        if existing_je:
            raise ConflictError(detail=f"Journal Entry with reference {journal_entry_data.reference_number} already exists for source {journal_entry_data.source_module}.", code="JE_REFERENCE_NUMBER_EXISTS")

    # --- NEW: Call Fraud Detection Service ---
    fraud_result = await _send_journal_entry_for_fraud_analysis(user_id, journal_entry_data, jwt_token)
    
    # Create JournalEntry node
    query_je = """
    MATCH (u:User {id: $user_id})
    CREATE (je:JournalEntry {
        id: $id,
        entry_date: datetime($entry_date),
        description: $description,
        reference_number: $reference_number,
        source_module: $source_module,
        status: $status,
        fraud_flag: $fraud_flag,
        fraud_score: toFloat($fraud_score),
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_JOURNAL_ENTRY]->(je)
    RETURN je
    """
    params_je = journal_entry_data.model_dump(exclude={"lines"})
    params_je["id"] = entry_neo4j_id
    params_je["user_id"] = user_id
    params_je["entry_date"] = params_je["entry_date"].isoformat()
    params_je["created_at"] = created_at.isoformat()
    params_je["updated_at"] = updated_at.isoformat()
    params_je["fraud_flag"] = fraud_result.fraud_flag # NEW
    params_je["fraud_score"] = fraud_result.fraud_score # NEW

    je_result = await session.run(query_je, params_je)
    je_node = (await je_result.single())["je"]

    # Create JournalLine nodes and link to JournalEntry and Account
    lines_in_db = []
    for line_data in journal_entry_data.lines:
        line_neo4j_id = str(uuid.uuid4())
        query_line = """
        MATCH (je:JournalEntry {id: $je_id})
        MATCH (a:Account {user_id: $user_id, account_number: $account_number})
        CREATE (jl:JournalLine {
            id: $id,
            debit: toFloat($debit),
            credit: toFloat($credit),
            description: $description
        })
        CREATE (je)-[:HAS_LINE]->(jl)
        CREATE (jl)-[:IMPACTS]->(a)
        RETURN jl
        """
        params_line = line_data.model_dump()
        params_line["id"] = line_neo4j_id
        params_line["je_id"] = entry_neo4j_id
        params_line["user_id"] = user_id
        params_line["debit"] = float(params_line["debit"])
        params_line["credit"] = float(params_line["credit"])

        jl_result = await session.run(query_line, params_line)
        jl_node = (await jl_result.single())["jl"]
        lines_in_db.append(JournalLineBase(
            account_number=line_data.account_number,
            debit=Decimal(str(jl_node["debit"])),
            credit=Decimal(str(jl_node["credit"])),
            description=jl_node["description"]
        ))

    return JournalEntryInDB(
        id=je_node["id"],
        user_id=user_id,
        entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
        description=je_node["description"],
        reference_number=je_node["reference_number"],
        source_module=je_node["source_module"],
        status=je_node["status"],
        lines=lines_in_db,
        fraud_flag=je_node["fraud_flag"],
        fraud_score=je_node["fraud_score"],
        created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
    )


async def get_journal_entry(session: AsyncSession, user_id: str, entry_id: str) -> Optional[JournalEntryInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry {id: $entry_id})
    OPTIONAL MATCH (je)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account)
    RETURN je, COLLECT({
        id: jl.id,
        account_number: a.account_number,
        debit: jl.debit,
        credit: jl.credit,
        description: jl.description
    }) AS lines_data
    """
    result = await session.run(query, user_id=user_id, entry_id=entry_id)
    record = await result.single()

    if record:
        je_node = record["je"]
        lines_data = record["lines_data"]
        
        lines_in_db = []
        for line_data in lines_data:
            if line_data and line_data.get("account_number"): # Check for valid line data
                lines_in_db.append(JournalLineBase(
                    account_number=line_data["account_number"],
                    debit=Decimal(str(line_data["debit"])),
                    credit=Decimal(str(line_data["credit"])),
                    description=line_data["description"]
                ))
        
        return JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            lines=lines_in_db,
            fraud_flag=je_node["fraud_flag"],
            fraud_score=je_node["fraud_score"],
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
        )
    return None

async def get_all_journal_entries(session: AsyncSession, user_id: str, start_date: Optional[datetime] = None, end_date: Optional[datetime] = None) -> List[JournalEntryInDB]:
    query_parts = []
    params = {"user_id": user_id}

    query_parts.append("MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)")

    if start_date:
        query_parts.append("WHERE je.entry_date >= datetime($start_date)")
        params["start_date"] = start_date.isoformat()
    if end_date:
        if "WHERE" not in query_parts[-1]:
            query_parts.append("WHERE je.entry_date <= datetime($end_date)")
        else:
            query_parts.append("AND je.entry_date <= datetime($end_date)")
        params["end_date"] = end_date.isoformat()

    query_parts.append("OPTIONAL MATCH (je)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account)")
    query_parts.append("RETURN je, COLLECT({id: jl.id, account_number: a.account_number, debit: jl.debit, credit: jl.credit, description: jl.description}) AS lines_data")
    query_parts.append("ORDER BY je.entry_date DESC")

    query = " ".join(query_parts)
    result = await session.run(query, params)

    entries_map: Dict[str, JournalEntryInDB] = {}

    async for record in result:
        je_node = record["je"]
        lines_data = record["lines_data"]
        entry_id = je_node["id"]

        if entry_id not in entries_map:
            entries_map[entry_id] = JournalEntryInDB(
                id=je_node["id"],
                user_id=user_id,
                entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
                description=je_node["description"],
                reference_number=je_node["reference_number"],
                source_module=je_node["source_module"],
                status=je_node["status"],
                lines=[],
                fraud_flag=je_node["fraud_flag"],
                fraud_score=je_node["fraud_score"],
                created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            )
        
        for line_data in lines_data:
            if line_data and line_data.get("account_number"):
                entries_map[entry_id].lines.append(JournalLineBase(
                    account_number=line_data["account_number"],
                    debit=Decimal(str(line_data["debit"])),
                    credit=Decimal(str(line_data["credit"])),
                    description=line_data["description"]
                ))
    
    return list(entries_map.values())

async def update_journal_entry(session: AsyncSession, user_id: str, entry_id: str, journal_entry_data: JournalEntryUpdate) -> Optional[JournalEntryInDB]:
    update_fields = journal_entry_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_journal_entry(session, user_id, entry_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "entry_date" in update_fields and update_fields["entry_date"]:
        update_fields["entry_date"] = update_fields["entry_date"].isoformat()

    set_clauses = [f"je.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry {{id: $entry_id}})
    SET {set_query_part}
    RETURN je
    """
    
    params = {"user_id": user_id, "entry_id": entry_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_journal_entry(session, user_id, entry_id)
    return None

async def delete_journal_entry(session: AsyncSession, user_id: str, entry_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry {id: $entry_id})
    OPTIONAL MATCH (je)-[:HAS_LINE]->(jl:JournalLine)
    DETACH DELETE je, jl
    """
    result = await session.run(query, user_id=user_id, entry_id=entry_id)
    return result.consume().counters.nodes_deleted > 0

# --- Helper to convert JournalEntry to TransactionForFraudCheck (NEW) ---
async def _send_journal_entry_for_fraud_analysis(user_id: str, journal_entry_data: JournalEntryCreate, jwt_token: str) -> FraudDetectionResult:
    """Internal helper to send journal entry data to Fraud Detection Service."""

    # Aggregate JE lines into a single pseudo-transaction for fraud analysis
    total_debit = sum(line.debit for line in journal_entry_data.lines)
    total_credit = sum(line.credit for line in journal_entry_data.lines)
    
    # For a balanced JE, total_debit == total_credits. Use one as the transaction amount.
    # The 'amount' in TransactionForFraudCheck refers to the value of the transaction.
    transaction_amount = total_debit 

    # Infer sender/recipient from common JE patterns or use placeholders
    # This is a simplification; a real system would need more context.
    sender_account = ""
    recipient_account = ""
    
    # Example: if a JE debits an expense and credits cash, cash is the source, expense is the destination
    # or vice-versa. For simplicity, we can use the first debit and credit accounts.
    debit_accounts = [line.account_number for line in journal_entry_data.lines if line.debit > 0]
    credit_accounts = [line.account_number for line in journal_entry_data.lines if line.credit > 0]

    if debit_accounts: sender_account = debit_accounts[0] # Often the 'source' of the value
    if credit_accounts: recipient_account = credit_accounts[0] # Often the 'destination' of the value

    transaction_type = "journal_entry" # Custom type for fraud service
    
    transaction_for_fraud = TransactionForFraudCheck(
        transaction_id=journal_entry_data.id if hasattr(journal_entry_data, 'id') else journal_entry_data.reference_number or str(uuid.uuid4()),
        amount=transaction_amount,
        currency="USD", # Assuming USD for now
        sender_account_id=sender_account,
        recipient_account_id=recipient_account,
        transaction_type=transaction_type,
        timestamp=journal_entry_data.entry_date,
        previous_transactions_count_24h=0, # No historical data from JE context
        avg_daily_transaction_amount_7d=Decimal('0.00') # No historical data from JE context
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
            print(f"Fraud Detection Service error for JE: {error_detail}")
            return FraudDetectionResult(
                transaction_id=transaction_for_fraud.transaction_id,
                fraud_score=0.0,
                fraud_flag="safe", # Default to safe if service fails
                reason=f"Fraud detection service failed for JE: {error_detail}",
                model_version="N/A_service_unavailable"
            )
        except httpx.RequestError as e:
            print(f"Network error communicating with Fraud Detection Service for JE: {e}")
            return FraudDetectionResult(
                transaction_id=transaction_for_fraud.transaction_id,
                fraud_score=0.0,
                fraud_flag="safe", # Default to safe if service unavailable
                reason=f"Network error connecting to Fraud Detection Service for JE: {e}",
                model_version="N/A_network_error"
            )

# --- Ledger Reports (unchanged) ---
# ...

# --- Trial Balance (unchanged) ---
# ...

# --- Financial Statements (unchanged) ---
# ...
