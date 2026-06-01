from neo4j import AsyncSession
from typing import Optional, List, Dict, Any, Tuple
from accounting_service.models import (
    AccountCreate, AccountUpdate, AccountInDB,
    JournalEntryCreate, JournalEntryUpdate, JournalEntryInDB, JournalLineBase,
    LedgerReport, LedgerEntry, TrialBalanceReport, TrialBalanceAccount,
    IncomeStatement, BalanceSheet, FinancialStatementLine,
    TransactionForFraudCheck, FraudDetectionResult,
    PurchaseOrderInDB, PurchaseOrderItemBase,
    VendorBillCreate, VendorBillInDB,
    # New Special Journals
    SalesJournalEntryCreate, SalesJournalEntryInDB,
    PurchasesJournalEntryCreate, PurchasesJournalEntryInDB,
    CashReceiptsJournalEntryCreate, CashReceiptsJournalEntryInDB,
    CashDisbursementsJournalEntryCreate, CashDisbursementsJournalEntryInDB,
    SalesReturnsJournalEntryCreate, SalesReturnsJournalEntryInDB,
    PurchasesReturnsJournalEntryCreate, PurchasesReturnsJournalEntryInDB,
    # Subsidiary Ledgers
    AccountsReceivableLedgerReport, AccountsReceivableLedgerEntry,
    AccountsPayableLedgerReport, AccountsPayableLedgerEntry,
    FixedAssetsLedgerReport, FixedAssetLedgerEntry,
    InventoryLedgerReport, InventoryLedgerEntry,
    # Petty Cash
    PettyCashFundCreate, PettyCashFundInDB, PettyCashEntryCreate, PettyCashEntryInDB,
    # Bank Reconciliation
    BankReconciliationStatement, BankReconciliationEntry
)
from datetime import datetime, timedelta
import uuid
from decimal import Decimal
import httpx
import os
from accounting_service.exceptions import ValidationError, NotFoundError, ConflictError

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# --- Account CRUD ---
async def create_account(session: AsyncSession, user_id: str, account_data: AccountCreate) -> AccountInDB:
    account_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Check for existing account number for this user
    existing_account = await get_account(session, user_id, account_data.account_number)
    if existing_account:
        raise ConflictError(detail=f"Account number {account_data.account_number} already exists for this user.", code="ACCOUNT_NUMBER_EXISTS")

    # Create Account node
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (a:Account {
        id: $id,
        name: $name,
        account_number: $account_number,
        account_type: $account_type,
        normal_balance: $normal_balance,
        description: $description,
        parent_account_number: $parent_account_number,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_ACCOUNT]->(a)
    RETURN a
    """
    params = account_data.model_dump()
    params["id"] = account_neo4j_id
    params["user_id"] = user_id
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    account_node = record["a"]

    return AccountInDB(
        id=account_node["id"],
        user_id=user_id,
        name=account_node["name"],
        account_number=account_node["account_number"],
        account_type=account_node["account_type"],
        normal_balance=account_node["normal_balance"],
        description=account_node["description"],
        parent_account_number=account_node["parent_account_number"],
        created_at=datetime.fromisoformat(account_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(account_node["updated_at"].iso_format()),
    )

async def get_account(session: AsyncSession, user_id: str, account_number: str) -> Optional[AccountInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ACCOUNT]->(a:Account {account_number: $account_number})
    RETURN a
    """
    result = await session.run(query, user_id=user_id, account_number=account_number)
    record = await result.single()

    if record:
        account_node = record["a"]
        return AccountInDB(
            id=account_node["id"],
            user_id=user_id,
            name=account_node["name"],
            account_number=account_node["account_number"],
            account_type=account_node["account_type"],
            normal_balance=account_node["normal_balance"],
            description=account_node["description"],
            parent_account_number=account_node["parent_account_number"],
            created_at=datetime.fromisoformat(account_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(account_node["updated_at"].iso_format()),
        )
    return None

async def get_all_accounts(session: AsyncSession, user_id: str) -> List[AccountInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ACCOUNT]->(a:Account)
    RETURN a
    ORDER BY a.account_number
    """
    result = await session.run(query, user_id=user_id)
    accounts = []
    async for record in result:
        account_node = record["a"]
        accounts.append(AccountInDB(
            id=account_node["id"],
            user_id=user_id,
            name=account_node["name"],
            account_number=account_node["account_number"],
            account_type=account_node["account_type"],
            normal_balance=account_node["normal_balance"],
            description=account_node["description"],
            parent_account_number=account_node["parent_account_number"],
            created_at=datetime.fromisoformat(account_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(account_node["updated_at"].iso_format()),
        ))
    return accounts

async def update_account(session: AsyncSession, user_id: str, account_number: str, account_data: AccountUpdate) -> Optional[AccountInDB]:
    update_fields = account_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_account(session, user_id, account_number) # No fields to update

    update_fields["updated_at"] = datetime.utcnow().isoformat()

    set_clauses = [f"a.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ACCOUNT]->(a:Account {{account_number: $account_number}})
    SET {set_query_part}
    RETURN a
    """
    
    params = {"user_id": user_id, "account_number": account_number, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_account(session, user_id, account_number)
    return None

async def delete_account(session: AsyncSession, user_id: str, account_number: str) -> bool:
    # Check if any journal lines are linked to this account
    check_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ACCOUNT]->(a:Account {account_number: $account_number})<-[:IMPACTS]-(:JournalLine)
    RETURN a LIMIT 1
    """
    check_result = await session.run(check_query, user_id=user_id, account_number=account_number)
    if await check_result.single():
        raise ConflictError(detail="Account is linked to existing journal entries and cannot be deleted.", code="ACCOUNT_LINKED")

    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ACCOUNT]->(a:Account {account_number: $account_number})
    DETACH DELETE a
    """
    result = await session.run(query, user_id=user_id, account_number=account_number)
    return result.consume().counters.nodes_deleted > 0

async def get_account_balance(session: AsyncSession, user_id: str, account_number: str, as_of_date: Optional[datetime] = None) -> Decimal:
    # Ensure account exists
    account = await get_account(session, user_id, account_number)
    if not account:
        raise NotFoundError(detail=f"Account {account_number} not found for user.", code="ACCOUNT_NOT_FOUND")

    date_filter = """
    AND je.entry_date <= datetime($as_of_date)
    """ if as_of_date else ""

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {{account_number: $account_number}})
    WHERE je.status = 'posted' {date_filter}
    RETURN SUM(jl.debit) AS total_debits, SUM(jl.credit) AS total_credits
    """
    params = {"user_id": user_id, "account_number": account_number}
    if as_of_date: params["as_of_date"] = as_of_date.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    total_debits = Decimal(str(record["total_debits"])) if record and record["total_debits"] else Decimal('0.00')
    total_credits = Decimal(str(record["total_credits"])) if record and record["total_credits"] else Decimal('0.00')

    balance = total_debits - total_credits if account.normal_balance == "debit" else total_credits - total_debits
    return balance

# --- NEW: Function to get account activity for a period --- (Added for Budget Variance Report)
async def get_account_period_activity(session: AsyncSession, user_id: str, account_number: str, start_date: datetime, end_date: datetime) -> Tuple[Decimal, Decimal]:
    """
    Calculates total debits and credits for a specific account within a given date range.
    """
    # Ensure account exists
    account = await get_account(session, user_id, account_number)
    if not account:
        raise NotFoundError(detail=f"Account {account_number} not found for user.", code="ACCOUNT_NOT_FOUND")

    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {account_number: $account_number})
    WHERE je.status = 'posted' AND je.entry_date >= datetime($start_date) AND je.entry_date <= datetime($end_date)
    RETURN SUM(jl.debit) AS total_debits, SUM(jl.credit) AS total_credits
    """
    params = {
        "user_id": user_id,
        "account_number": account_number,
        "start_date": start_date.isoformat(),
        "end_date": end_date.isoformat()
    }

    result = await session.run(query, params)
    record = await result.single()

    total_debits = Decimal(str(record["total_debits"])) if record and record["total_debits"] else Decimal('0.00')
    total_credits = Decimal(str(record["total_credits"])) if record and record["total_credits"] else Decimal('0.00')

    return total_debits, total_credits


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
    if journal_entry_data.reference_number and journal_entry_data.source_module in ["Banking", "Invoicing", "Multimodal", "SupplyChain"]:
        existing_je = await get_journal_entry_by_reference(session, user_id, journal_entry_data.reference_number, journal_entry_data.source_module)
        if existing_je:
            raise ConflictError(detail=f"Journal Entry with reference {journal_entry_data.reference_number} already exists for source {journal_entry_data.source_module}.", code="JE_REFERENCE_NUMBER_EXISTS")

    # --- NEW: Call Fraud Detection Service ---
    fraud_result = await _send_journal_entry_for_fraud_analysis(user_id, journal_entry_data, jwt_token)
    
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (je:JournalEntry {
        id: $id,
        entry_date: datetime($entry_date),
        description: $description,
        reference_number: $reference_number,
        source_module: $source_module,
        status: $status,
        fraud_flag: $fraud_flag,
        fraud_score: $fraud_score,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_JOURNAL_ENTRY]->(je)
    """
    # Create JournalLine nodes and link them
    for i, line in enumerate(journal_entry_data.lines):
        line_id = str(uuid.uuid4())
        query += f"""
        MATCH (je_match:JournalEntry {{id: $id}}), (a_match:Account {{account_number: $line_{i}_account_number}})
        CREATE (jl_{i}:JournalLine {{
            id: $line_{i}_id,
            debit: toFloat($line_{i}_debit),
            credit: toFloat($line_{i}_credit),
            description: $line_{i}_description,
            created_at: datetime($created_at),
            updated_at: datetime($updated_at)
        }})
        CREATE (je_match)-[:HAS_LINE]->(jl_{i})
        CREATE (jl_{i})-[:IMPACTS]->(a_match)
        """
        params[f"line_{i}_id"] = line_id
        params[f"line_{i}_account_number"] = line.account_number
        params[f"line_{i}_debit"] = float(line.debit)
        params[f"line_{i}_credit"] = float(line.credit)
        params[f"line_{i}_description"] = line.description
    
    query += " RETURN je"

    params = {
        "id": entry_neo4j_id,
        "user_id": user_id,
        "entry_date": journal_entry_data.entry_date.isoformat(),
        "description": journal_entry_data.description,
        "reference_number": journal_entry_data.reference_number,
        "source_module": journal_entry_data.source_module,
        "status": journal_entry_data.status,
        "fraud_flag": fraud_result.fraud_flag,
        "fraud_score": fraud_result.fraud_score,
        "created_at": created_at.isoformat(),
        "updated_at": updated_at.isoformat(),
        **{f"line_{i}_id": str(uuid.uuid4()) for i in range(len(journal_entry_data.lines))}, # Initialize unique IDs
        **{f"line_{i}_account_number": line.account_number for i, line in enumerate(journal_entry_data.lines)},
        **{f"line_{i}_debit": float(line.debit) for i, line in enumerate(journal_entry_data.lines)},
        **{f"line_{i}_credit": float(line.credit) for i, line in enumerate(journal_entry_data.lines)},
        **{f"line_{i}_description": line.description for i, line in enumerate(journal_entry_data.lines)},
    }
    
    result = await session.run(query, params)
    record = await result.single()
    je_node = record["je"]

    # Reconstruct JournalEntryInDB with lines (more complex query needed if full lines are to be returned from initial creation)
    # For now, this will fetch the created JE without detailed lines from this query.
    # A subsequent `get_journal_entry` call would be needed for full details.
    return JournalEntryInDB(
        id=je_node["id"],
        user_id=user_id,
        entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
        description=je_node["description"],
        reference_number=je_node["reference_number"],
        source_module=je_node["source_module"],
        status=je_node["status"],
        lines=journal_entry_data.lines, # This is a placeholder; actual lines need to be retrieved from DB
        fraud_flag=je_node["fraud_flag"],
        fraud_score=je_node["fraud_score"],
        created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
    )

async def get_journal_entry(session: AsyncSession, user_id: str, entry_id: str) -> Optional[JournalEntryInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry {id: $entry_id})
    OPTIONAL MATCH (je)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account)
    RETURN je, COLLECT({line: jl, account: a}) AS lines_data
    """
    result = await session.run(query, user_id=user_id, entry_id=entry_id)
    record = await result.single()

    if record:
        je_node = record["je"]
        lines_data = record["lines_data"]
        
        journal_lines = []
        for line_item in lines_data:
            if line_item["line"] and line_item["account"]:
                journal_lines.append(JournalLineBase(
                    account_number=line_item["account"]["account_number"],
                    debit=Decimal(str(line_item["line"]["debit"])),
                    credit=Decimal(str(line_item["line"]["credit"])),
                    description=line_item["line"]["description"]
                ))

        return JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            lines=journal_lines,
            fraud_flag=je_node["fraud_flag"],
            fraud_score=je_node["fraud_score"],
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
        )
    return None

async def get_journal_entry_by_reference(session: AsyncSession, user_id: str, reference_number: str, source_module: str) -> Optional[JournalEntryInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry {reference_number: $reference_number, source_module: $source_module})
    RETURN je
    """
    result = await session.run(query, user_id=user_id, reference_number=reference_number, source_module=source_module)
    record = await result.single()

    if record:
        je_node = record["je"]
        return JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            lines=[], # Lines are not eagerly loaded by this simple query
            fraud_flag=je_node["fraud_flag"],
            fraud_score=je_node["fraud_score"],
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
        )
    return None

async def get_all_journal_entries(session: AsyncSession, user_id: str, start_date: Optional[datetime] = None, end_date: Optional[datetime] = None) -> List[JournalEntryInDB]:
    date_filter = ""
    params = {"user_id": user_id}
    if start_date:
        date_filter += " AND je.entry_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND je.entry_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)
    OPTIONAL MATCH (je)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account)
    WHERE true {date_filter}
    RETURN je, COLLECT({{line: jl, account: a}}) AS lines_data
    ORDER BY je.entry_date DESC, je.created_at DESC
    """
    result = await session.run(query, params)

    journal_entries_map: Dict[str, JournalEntryInDB] = {}

    async for record in result:
        je_node = record["je"]
        entry_id = je_node["id"]
        lines_data = record["lines_data"]

        if entry_id not in journal_entries_map:
            journal_entries_map[entry_id] = JournalEntryInDB(
                id=je_node["id"],
                user_id=user_id,
                entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
                description=je_node["description"],
                reference_number=je_node["reference_number"],
                source_module=je_node["source_module"],
                status=je_node["status"],
                lines=[], # Will populate below
                fraud_flag=je_node["fraud_flag"],
                fraud_score=je_node["fraud_score"],
                created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            )
        
        for line_item in lines_data:
            if line_item["line"] and line_item["account"]:
                journal_entries_map[entry_id].lines.append(JournalLineBase(
                    account_number=line_item["account"]["account_number"],
                    debit=Decimal(str(line_item["line"]["debit"])),
                    credit=Decimal(str(line_item["line"]["credit"])),
                    description=line_item["line"]["description"]
                ))
    
    return list(journal_entries_map.values())


async def update_journal_entry(session: AsyncSession, user_id: str, entry_id: str, journal_entry_data: JournalEntryUpdate) -> Optional[JournalEntryInDB]:
    update_fields = journal_entry_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_journal_entry(session, user_id, entry_id) # No fields to update

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "entry_date" in update_fields:
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

# --- Fraud Detection Integration ---
async def _send_journal_entry_for_fraud_analysis(user_id: str, journal_entry_data: JournalEntryCreate, jwt_token: str) -> FraudDetectionResult:
    """
    Internal function to send journal entry data to the Fraud Detection Service for analysis.
    """
    fraud_service_url = f"{API_GATEWAY_URL}/fraud-detection/analyze"
    
    # Prepare a simplified TransactionForFraudCheck model from JournalEntryCreate
    # This requires making some assumptions or having more detailed info in JE.
    # For now, we take the first line as representative for transaction amount/type.
    # In a real scenario, this would be more complex, potentially involving multiple transactions.
    if not journal_entry_data.lines:
        return FraudDetectionResult(transaction_id="N/A", fraud_score=0.0, fraud_flag="safe", reason="No lines in entry.")

    first_line = journal_entry_data.lines[0]
    transaction_amount = first_line.debit if first_line.debit > 0 else first_line.credit
    transaction_type = "debit" if first_line.debit > 0 else "credit"
    
    # Placeholder for sender/recipient accounts - needs actual logic to determine from JE context
    sender_account_id = journal_entry_data.lines[0].account_number if journal_entry_data.lines[0].debit > 0 else "unknown"
    recipient_account_id = journal_entry_data.lines[0].account_number if journal_entry_data.lines[0].credit > 0 else "unknown"

    transaction_for_fraud_check = TransactionForFraudCheck(
        transaction_id=str(uuid.uuid4()), # Generate a new ID for the fraud check transaction
        amount=transaction_amount,
        currency=journal_entry_data.currency if hasattr(journal_entry_data, 'currency') else "USD", # Assuming currency in JE
        sender_account_id=sender_account_id, # Simplified
        recipient_account_id=recipient_account_id, # Simplified
        transaction_type=transaction_type,
        timestamp=journal_entry_data.entry_date,
        location_data={"ip_address": "127.0.0.1"}, # Placeholder
        device_info={"user_agent": "FinAcc-AccountingService"}, # Placeholder
    )

    headers = {"Authorization": f"Bearer {jwt_token}", "Content-Type": "application/json"}
    try:
        async with httpx.AsyncClient() as client:
            response = await client.post(fraud_service_url, json=transaction_for_fraud_check.model_dump_json(), headers=headers)
            response.raise_for_status()
            return FraudDetectionResult(**response.json())
    except httpx.HTTPStatusError as e:
        print(f"Fraud Detection Service failed: {e.response.status_code} - {e.response.text}")
        return FraudDetectionResult(transaction_id=transaction_for_fraud_check.transaction_id, fraud_score=0.0, fraud_flag="safe", reason=f"Service error: {e.response.status_code}")
    except httpx.RequestError as e:
        print(f"Fraud Detection Service network error: {e}")
        return FraudDetectionResult(transaction_id=transaction_for_fraud_check.transaction_id, fraud_score=0.0, fraud_flag="safe", reason=f"Network error: {e}")


# --- Vendor Bill CRUD ---
async def create_vendor_bill(session: AsyncSession, user_id: str, vendor_bill_data: VendorBillCreate, jwt_token: str) -> VendorBillInDB:
    bill_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Create VendorBill node
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (vb:VendorBill {
        id: $id,
        vendor_id: $vendor_id,
        bill_number: $bill_number,
        invoice_date: datetime($invoice_date),
        due_date: datetime($due_date),
        total_amount: toFloat($total_amount),
        currency: $currency,
        status: $status,
        description: $description,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_VENDOR_BILL]->(vb)
    """
    
    params = vendor_bill_data.model_dump()
    params["id"] = bill_neo4j_id
    params["user_id"] = user_id
    params["invoice_date"] = params["invoice_date"].isoformat()
    params["due_date"] = params["due_date"].isoformat()
    params["total_amount"] = float(params["total_amount"])
    params["currency"] = params["currency"]
    params["status"] = params["status"]
    params["description"] = params["description"]
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    # Link to purchase orders if provided
    if vendor_bill_data.linked_purchase_order_ids:
        for i, po_id in enumerate(vendor_bill_data.linked_purchase_order_ids):
            query += f"""
            MATCH (po{i}:PurchaseOrder {{id: $po_id_{i}}}), (vb_match:VendorBill {{id: $id}})
            CREATE (vb_match)-[:LINKED_TO_PO]->(po{i})
            """
            params[f"po_id_{i}"] = po_id

    # Create corresponding Journal Entry
    # This assumes appropriate accounts exist (Accounts Payable, Expense Account)
    # The actual account numbers would need to be passed or derived
    accounts_payable_account_number = "2000" # Example
    expense_account_number = "6000" # Example

    journal_lines = [
        JournalLineBase(account_number=expense_account_number, debit=vendor_bill_data.total_amount, credit=Decimal('0.00'), description=f"Vendor Bill {vendor_bill_data.bill_number}"),
        JournalLineBase(account_number=accounts_payable_account_number, debit=Decimal('0.00'), credit=vendor_bill_data.total_amount, description=f"Vendor Bill {vendor_bill_data.bill_number}")
    ]
    journal_entry_data = JournalEntryCreate(
        entry_date=datetime.utcnow(),
        description=f"Auto-generated JE for Vendor Bill {vendor_bill_data.bill_number}",
        reference_number=vendor_bill_data.bill_number,
        source_module="VendorBilling",
        lines=journal_lines,
        status="posted"
    )
    
    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token) # Pass jwt_token

    query += """
    MATCH (vb_match:VendorBill {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (vb_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    RETURN vb
    """
    params["journal_entry_id"] = created_je.id

    result = await session.run(query, params)
    record = await result.single()
    vb_node = record["vb"]

    return VendorBillInDB(
        id=vb_node["id"],
        user_id=user_id,
        vendor_id=vb_node["vendor_id"],
        bill_number=vb_node["bill_number"],
        invoice_date=datetime.fromisoformat(vb_node["invoice_date"].iso_format()),
        due_date=datetime.fromisoformat(vb_node["due_date"].iso_format()),
        total_amount=Decimal(str(vb_node["total_amount"])),
        currency=vb_node["currency"],
        status=vb_node["status"],
        description=vb_node["description"],
        linked_purchase_order_ids=vendor_bill_data.linked_purchase_order_ids,
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(vb_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(vb_node["updated_at"].iso_format()),
    )
    
# --- Ledger Report ---
async def get_ledger_report(session: AsyncSession, user_id: str, account_number: str, start_date: Optional[datetime] = None, end_date: Optional[datetime] = None) -> LedgerReport:
    account = await get_account(session, user_id, account_number)
    if not account:
        raise NotFoundError(detail=f"Account {account_number} not found for user.", code="ACCOUNT_NOT_FOUND")

    # Get initial balance before start_date
    initial_balance = await get_account_balance(session, user_id, account_number, as_of_date=(start_date - timedelta(microseconds=1) if start_date else None))

    date_filter = ""
    params = {"user_id": user_id, "account_number": account_number}
    if start_date:
        date_filter += " AND je.entry_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND je.entry_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {{account_number: $account_number}})
    WHERE je.status = 'posted' {date_filter}
    RETURN je, jl
    ORDER BY je.entry_date ASC, je.created_at ASC
    """
    result = await session.run(query, params)

    entries = []
    current_balance = initial_balance

    async for record in result:
        jl_node = record["jl"]
        je_node = record["je"]
        debit = Decimal(str(jl_node["debit"]))
        credit = Decimal(str(jl_node["credit"]))

        if account.normal_balance == "debit":
            current_balance += debit - credit
        else: # normal_balance == "credit"
            current_balance += credit - debit
        
        entries.append(
            LedgerEntry(
                entry_id=je_node["id"],
                entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
                description=je_node["description"],
                debit=debit,
                credit=credit,
                balance=current_balance,
                source_module=je_node["source_module"]
            )
        )
    
    end_balance = await get_account_balance(session, user_id, account_number, as_of_date=end_date) if end_date else current_balance

    return LedgerReport(
        account_number=account_number,
        account_name=account.name,
        normal_balance=account.normal_balance,
        start_balance=initial_balance,
        entries=entries,
        end_balance=end_balance
    )

# --- Trial Balance Report ---
async def get_trial_balance_report(session: AsyncSession, user_id: str, as_of_date: Optional[datetime] = None) -> TrialBalanceReport:
    all_accounts = await get_all_accounts(session, user_id)
    
    trial_balance_accounts = []
    total_debits = Decimal('0.00')
    total_credits = Decimal('0.00')

    for account in all_accounts:
        current_balance = await get_account_balance(session, user_id, account.account_number, as_of_date)
        
        tb_account_debit = Decimal('0.00')
        tb_account_credit = Decimal('0.00')

        if account.normal_balance == "debit":
            if current_balance >= 0:
                tb_account_debit = current_balance
            else:
                tb_account_credit = abs(current_balance) # Negative debit balance is a credit
        else: # normal_balance == "credit"
            if current_balance >= 0:
                tb_account_credit = current_balance
            else:
                tb_account_debit = abs(current_balance) # Negative credit balance is a debit
        
        total_debits += tb_account_debit
        total_credits += tb_account_credit

        trial_balance_accounts.append(
            TrialBalanceAccount(
                account_number=account.account_number,
                account_name=account.name,
                account_type=account.account_type,
                debit=tb_account_debit,
                credit=tb_account_credit
            )
        )
    
    is_balanced = (total_debits == total_credits)

    return TrialBalanceReport(
        report_date=as_of_date if as_of_date else datetime.utcnow(),
        accounts=trial_balance_accounts,
        total_debits=total_debits,
        total_credits=total_credits,
        is_balanced=is_balanced
    )

# --- Income Statement Report ---
async def get_income_statement(session: AsyncSession, user_id: str, start_date: datetime, end_date: datetime) -> IncomeStatement:
    # Revenue accounts (credit normal) and Expense accounts (debit normal)
    revenue_accounts = await get_all_accounts_by_type(session, user_id, "revenue")
    expense_accounts = await get_all_accounts_by_type(session, user_id, "expense")

    revenues: List[FinancialStatementLine] = []
    expenses: List[FinancialStatementLine] = []
    total_revenue = Decimal('0.00')
    total_expense = Decimal('0.00')

    # Calculate revenues
    for account in revenue_accounts:
        # For income statement, we need the *activity* during the period, not cumulative balance.
        # Revenue increases with credits.
        debits, credits = await get_account_period_activity(session, user_id, account.account_number, start_date, end_date)
        net_credits = credits - debits
        if net_credits > 0:
            revenues.append(FinancialStatementLine(category=account.name, amount=net_credits))
            total_revenue += net_credits
    
    # Calculate expenses
    for account in expense_accounts:
        # Expense increases with debits.
        debits, credits = await get_account_period_activity(session, user_id, account.account_number, start_date, end_date)
        net_debits = debits - credits
        if net_debits > 0:
            expenses.append(FinancialStatementLine(category=account.name, amount=net_debits))
            total_expense += net_debits
    
    net_income = total_revenue - total_expense

    return IncomeStatement(
        start_date=start_date,
        end_date=end_date,
        revenues=revenues,
        expenses=expenses,
        net_income=net_income
    )

async def get_all_accounts_by_type(session: AsyncSession, user_id: str, account_type: str) -> List[AccountInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ACCOUNT]->(a:Account {account_type: $account_type})
    RETURN a
    ORDER BY a.account_number
    """
    result = await session.run(query, user_id=user_id, account_type=account_type)
    accounts = []
    async for record in result:
        account_node = record["a"]
        accounts.append(AccountInDB(
            id=account_node["id"],
            user_id=user_id,
            name=account_node["name"],
            account_number=account_node["account_number"],
            account_type=account_node["account_type"],
            normal_balance=account_node["normal_balance"],
            description=account_node["description"],
            parent_account_number=account_node["parent_account_number"],
            created_at=datetime.fromisoformat(account_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(account_node["updated_at"].iso_format()),
        ))
    return accounts


# --- Balance Sheet Report ---
async def get_balance_sheet(session: AsyncSession, user_id: str, as_of_date: datetime) -> BalanceSheet:
    asset_accounts = await get_all_accounts_by_type(session, user_id, "asset")
    liability_accounts = await get_all_accounts_by_type(session, user_id, "liability")
    equity_accounts = await get_all_accounts_by_type(session, user_id, "equity")

    assets: List[FinancialStatementLine] = []
    liabilities: List[FinancialStatementLine] = []
    equity: List[FinancialStatementLine] = []

    total_assets = Decimal('0.00')
    total_liabilities = Decimal('0.00')
    total_equity = Decimal('0.00')

    for account in asset_accounts:
        balance = await get_account_balance(session, user_id, account.account_number, as_of_date)
        assets.append(FinancialStatementLine(category=account.name, amount=balance))
        total_assets += balance
    
    for account in liability_accounts:
        balance = await get_account_balance(session, user_id, account.account_number, as_of_date)
        liabilities.append(FinancialStatementLine(category=account.name, amount=balance))
        total_liabilities += balance

    for account in equity_accounts:
        balance = await get_account_balance(session, user_id, account.account_number, as_of_date)
        equity.append(FinancialStatementLine(category=account.name, amount=balance))
        total_equity += balance
    
    total_liabilities_equity = total_liabilities + total_equity
    
    return BalanceSheet(
        as_of_date=as_of_date,
        assets=assets,
        liabilities=liabilities,
        equity=equity,
        total_assets=total_assets,
        total_liabilities=total_liabilities,
        total_equity=total_equity,
        total_liabilities_equity=total_liabilities_equity
    )

# --- Vendor Bill CRUD ---
async def create_vendor_bill(session: AsyncSession, user_id: str, vendor_bill_data: VendorBillCreate, jwt_token: str) -> VendorBillInDB:
    bill_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Create VendorBill node
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (vb:VendorBill {
        id: $id,
        vendor_id: $vendor_id,
        bill_number: $bill_number,
        invoice_date: datetime($invoice_date),
        due_date: datetime($due_date),
        total_amount: toFloat($total_amount),
        currency: $currency,
        status: $status,
        description: $description,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_VENDOR_BILL]->(vb)
    """
    
    params = vendor_bill_data.model_dump()
    params["id"] = bill_neo4j_id
    params["user_id"] = user_id
    params["invoice_date"] = params["invoice_date"].isoformat()
    params["due_date"] = params["due_date"].isoformat()
    params["total_amount"] = float(params["total_amount"])
    params["currency"] = params["currency"]
    params["status"] = params["status"]
    params["description"] = params["description"]
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    # Link to purchase orders if provided
    if vendor_bill_data.linked_purchase_order_ids:
        for i, po_id in enumerate(vendor_bill_data.linked_purchase_order_ids):
            query += f"""
            MATCH (po{i}:PurchaseOrder {{id: $po_id_{i}}}), (vb_match:VendorBill {{id: $id}})
            CREATE (vb_match)-[:LINKED_TO_PO]->(po{i})
            """
            params[f"po_id_{i}"] = po_id

    # Create corresponding Journal Entry
    # This assumes appropriate accounts exist (Accounts Payable, Expense Account)
    # The actual account numbers would need to be passed or derived
    accounts_payable_account_number = "2000" # Example
    expense_account_number = "6000" # Example

    journal_lines = [
        JournalLineBase(account_number=expense_account_number, debit=vendor_bill_data.total_amount, credit=Decimal('0.00'), description=f"Vendor Bill {vendor_bill_data.bill_number}"),
        JournalLineBase(account_number=accounts_payable_account_number, debit=Decimal('0.00'), credit=vendor_bill_data.total_amount, description=f"Vendor Bill {vendor_bill_data.bill_number}")
    ]
    journal_entry_data = JournalEntryCreate(
        entry_date=datetime.utcnow(),
        description=f"Auto-generated JE for Vendor Bill {vendor_bill_data.bill_number}",
        reference_number=vendor_bill_data.bill_number,
        source_module="VendorBilling",
        lines=journal_lines,
        status="posted"
    )
    
    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token) # Pass jwt_token

    query += """
    MATCH (vb_match:VendorBill {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (vb_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    RETURN vb
    """
    params["journal_entry_id"] = created_je.id

    result = await session.run(query, params)
    record = await result.single()
    vb_node = record["vb"]

    return VendorBillInDB(
        id=vb_node["id"],
        user_id=user_id,
        vendor_id=vb_node["vendor_id"],
        bill_number=vb_node["bill_number"],
        invoice_date=datetime.fromisoformat(vb_node["invoice_date"].iso_format()),
        due_date=datetime.fromisoformat(vb_node["due_date"].iso_format()),
        total_amount=Decimal(str(vb_node["total_amount"])),
        currency=vb_node["currency"],
        status=vb_node["status"],
        description=vb_node["description"],
        linked_purchase_order_ids=vendor_bill_data.linked_purchase_order_ids,
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(vb_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(vb_node["updated_at"].iso_format()),
    )
    
# --- Ledger Report (unchanged) ---
# --- Trial Balance Report (unchanged) ---
# --- Income Statement Report (unchanged) ---
# --- Balance Sheet Report (unchanged) ---


# ============================================================
# SPECIAL JOURNALS CRUD (Books of Original Entry)
# ============================================================

async def create_sales_journal_entry(session: AsyncSession, user_id: str, entry: SalesJournalEntryCreate, jwt_token: str) -> SalesJournalEntryInDB:
    """Create Sales Journal Entry and auto-generate journal entry"""
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    # Create Sales Journal node
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (sj:SalesJournalEntry {
        id: $id,
        invoice_number: $invoice_number,
        customer_id: $customer_id,
        invoice_date: datetime($invoice_date),
        due_date: datetime($due_date),
        total_amount: toFloat($total_amount),
        tax_amount: toFloat($tax_amount),
        discount_amount: toFloat($discount_amount),
        currency: $currency,
        status: $status,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_SALES_JOURNAL_ENTRY]->(sj)
    RETURN sj
    """

    params = {
        "id": entry_id, "user_id": user_id,
        "invoice_number": entry.invoice_number, "customer_id": entry.customer_id,
        "invoice_date": entry.invoice_date.isoformat(),
        "due_date": entry.due_date.isoformat() if entry.due_date else None,
        "total_amount": float(entry.total_amount), "tax_amount": float(entry.tax_amount),
        "discount_amount": float(entry.discount_amount), "currency": entry.currency,
        "status": entry.status, "notes": entry.notes,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }

    # Auto-generate journal entry
    net_amount = entry.total_amount - entry.discount_amount
    sales_revenue_account = "4000"  # Sales Revenue
    accounts_receivable_account = "1100"  # Accounts Receivable

    journal_lines = [
        JournalLineBase(account_number=accounts_receivable_account, debit=net_amount, credit=Decimal('0.00'), description=f"Sales Invoice {entry.invoice_number}"),
        JournalLineBase(account_number=sales_revenue_account, debit=Decimal('0.00'), credit=net_amount, description=f"Sales Invoice {entry.invoice_number}")
    ]

    journal_entry_data = JournalEntryCreate(
        entry_date=entry.invoice_date,
        description=f"Sales Journal: Invoice {entry.invoice_number}",
        reference_number=entry.invoice_number,
        source_module="SalesJournal",
        lines=journal_lines,
        status="posted"
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token)
    params["journal_entry_id"] = created_je.id

    query += """
    MATCH (sj_match:SalesJournalEntry {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (sj_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    """

    result = await session.run(query, params)
    record = await result.single()
    sj_node = record["sj"]

    return SalesJournalEntryInDB(
        id=sj_node["id"], user_id=user_id,
        invoice_number=sj_node["invoice_number"], customer_id=sj_node["customer_id"],
        invoice_date=datetime.fromisoformat(sj_node["invoice_date"].iso_format()),
        due_date=datetime.fromisoformat(sj_node["due_date"].iso_format()) if sj_node["due_date"] else None,
        total_amount=Decimal(str(sj_node["total_amount"])),
        tax_amount=Decimal(str(sj_node["tax_amount"])),
        discount_amount=Decimal(str(sj_node["discount_amount"])),
        currency=sj_node["currency"], status=sj_node["status"], notes=sj_node["notes"],
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(sj_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(sj_node["updated_at"].iso_format())
    )

async def get_sales_journal_entries(session: AsyncSession, user_id: str, start_date, end_date, customer_id, status) -> List[SalesJournalEntryInDB]:
    date_filter = ""
    params = {"user_id": user_id}
    if start_date:
        date_filter += " AND sj.invoice_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND sj.invoice_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()
    if customer_id:
        date_filter += " AND sj.customer_id = $customer_id"
        params["customer_id"] = customer_id
    if status:
        date_filter += " AND sj.status = $status"
        params["status"] = status

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SALES_JOURNAL_ENTRY]->(sj:SalesJournalEntry)
    WHERE true {date_filter}
    OPTIONAL MATCH (sj)-[:GENERATED_JOURNAL_ENTRY]->(je:JournalEntry)
    RETURN sj, je.id as journal_entry_id
    ORDER BY sj.invoice_date DESC
    """
    result = await session.run(query, params)
    entries = []
    async for record in result:
        sj = record["sj"]
        entries.append(SalesJournalEntryInDB(
            id=sj["id"], user_id=user_id,
            invoice_number=sj["invoice_number"], customer_id=sj["customer_id"],
            invoice_date=datetime.fromisoformat(sj["invoice_date"].iso_format()),
            due_date=datetime.fromisoformat(sj["due_date"].iso_format()) if sj["due_date"] else None,
            total_amount=Decimal(str(sj["total_amount"])),
            tax_amount=Decimal(str(sj["tax_amount"])),
            discount_amount=Decimal(str(sj["discount_amount"])),
            currency=sj["currency"], status=sj["status"], notes=sj["notes"],
            journal_entry_id=record["journal_entry_id"],
            created_at=datetime.fromisoformat(sj["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(sj["updated_at"].iso_format())
        ))
    return entries

async def get_sales_journal_entry(session: AsyncSession, user_id: str, entry_id: str) -> SalesJournalEntryInDB:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SALES_JOURNAL_ENTRY]->(sj:SalesJournalEntry {id: $entry_id})
    OPTIONAL MATCH (sj)-[:GENERATED_JOURNAL_ENTRY]->(je:JournalEntry)
    RETURN sj, je.id as journal_entry_id
    """
    result = await session.run(query, user_id=user_id, entry_id=entry_id)
    record = await result.single()
    if not record:
        raise NotFoundError(detail="Sales journal entry not found")
    sj = record["sj"]
    return SalesJournalEntryInDB(
        id=sj["id"], user_id=user_id,
        invoice_number=sj["invoice_number"], customer_id=sj["customer_id"],
        invoice_date=datetime.fromisoformat(sj["invoice_date"].iso_format()),
        due_date=datetime.fromisoformat(sj["due_date"].iso_format()) if sj["due_date"] else None,
        total_amount=Decimal(str(sj["total_amount"])),
        tax_amount=Decimal(str(sj["tax_amount"])),
        discount_amount=Decimal(str(sj["discount_amount"])),
        currency=sj["currency"], status=sj["status"], notes=sj["notes"],
        journal_entry_id=record["journal_entry_id"],
        created_at=datetime.fromisoformat(sj["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(sj["updated_at"].iso_format())
    )

async def create_purchases_journal_entry(session: AsyncSession, user_id: str, entry: PurchasesJournalEntryCreate, jwt_token: str) -> PurchasesJournalEntryInDB:
    """Create Purchases Journal Entry and auto-generate journal entry"""
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (pj:PurchasesJournalEntry {
        id: $id,
        purchase_order_number: $purchase_order_number,
        vendor_id: $vendor_id,
        purchase_date: datetime($purchase_date),
        due_date: datetime($due_date),
        total_amount: toFloat($total_amount),
        tax_amount: toFloat($tax_amount),
        discount_amount: toFloat($discount_amount),
        currency: $currency,
        status: $status,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_PURCHASES_JOURNAL_ENTRY]->(pj)
    RETURN pj
    """

    params = {
        "id": entry_id, "user_id": user_id,
        "purchase_order_number": entry.purchase_order_number, "vendor_id": entry.vendor_id,
        "purchase_date": entry.purchase_date.isoformat(),
        "due_date": entry.due_date.isoformat() if entry.due_date else None,
        "total_amount": float(entry.total_amount), "tax_amount": float(entry.tax_amount),
        "discount_amount": float(entry.discount_amount), "currency": entry.currency,
        "status": entry.status, "notes": entry.notes,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }

    # Auto-generate journal entry
    net_amount = entry.total_amount - entry.discount_amount
    inventory_account = "1200"  # Inventory
    accounts_payable_account = "2000"  # Accounts Payable

    journal_lines = [
        JournalLineBase(account_number=inventory_account, debit=net_amount, credit=Decimal('0.00'), description=f"Purchase {entry.purchase_order_number}"),
        JournalLineBase(account_number=accounts_payable_account, debit=Decimal('0.00'), credit=net_amount, description=f"Purchase {entry.purchase_order_number}")
    ]

    journal_entry_data = JournalEntryCreate(
        entry_date=entry.purchase_date,
        description=f"Purchases Journal: PO {entry.purchase_order_number}",
        reference_number=entry.purchase_order_number,
        source_module="PurchasesJournal",
        lines=journal_lines,
        status="posted"
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token)
    params["journal_entry_id"] = created_je.id

    query += """
    MATCH (pj_match:PurchasesJournalEntry {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (pj_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    """

    result = await session.run(query, params)
    record = await result.single()
    pj_node = record["pj"]

    return PurchasesJournalEntryInDB(
        id=pj_node["id"], user_id=user_id,
        purchase_order_number=pj_node["purchase_order_number"], vendor_id=pj_node["vendor_id"],
        purchase_date=datetime.fromisoformat(pj_node["purchase_date"].iso_format()),
        due_date=datetime.fromisoformat(pj_node["due_date"].iso_format()) if pj_node["due_date"] else None,
        total_amount=Decimal(str(pj_node["total_amount"])),
        tax_amount=Decimal(str(pj_node["tax_amount"])),
        discount_amount=Decimal(str(pj_node["discount_amount"])),
        currency=pj_node["currency"], status=pj_node["status"], notes=pj_node["notes"],
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(pj_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(pj_node["updated_at"].iso_format())
    )

async def get_purchases_journal_entries(session: AsyncSession, user_id: str, start_date, end_date, vendor_id, status) -> List[PurchasesJournalEntryInDB]:
    date_filter = ""
    params = {"user_id": user_id}
    if start_date:
        date_filter += " AND pj.purchase_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND pj.purchase_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()
    if vendor_id:
        date_filter += " AND pj.vendor_id = $vendor_id"
        params["vendor_id"] = vendor_id
    if status:
        date_filter += " AND pj.status = $status"
        params["status"] = status

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PURCHASES_JOURNAL_ENTRY]->(pj:PurchasesJournalEntry)
    WHERE true {date_filter}
    OPTIONAL MATCH (pj)-[:GENERATED_JOURNAL_ENTRY]->(je:JournalEntry)
    RETURN pj, je.id as journal_entry_id
    ORDER BY pj.purchase_date DESC
    """
    result = await session.run(query, params)
    entries = []
    async for record in result:
        pj = record["pj"]
        entries.append(PurchasesJournalEntryInDB(
            id=pj["id"], user_id=user_id,
            purchase_order_number=pj["purchase_order_number"], vendor_id=pj["vendor_id"],
            purchase_date=datetime.fromisoformat(pj["purchase_date"].iso_format()),
            due_date=datetime.fromisoformat(pj["due_date"].iso_format()) if pj["due_date"] else None,
            total_amount=Decimal(str(pj["total_amount"])),
            tax_amount=Decimal(str(pj["tax_amount"])),
            discount_amount=Decimal(str(pj["discount_amount"])),
            currency=pj["currency"], status=pj["status"], notes=pj["notes"],
            journal_entry_id=record["journal_entry_id"],
            created_at=datetime.fromisoformat(pj["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(pj["updated_at"].iso_format())
        ))
    return entries

async def create_cash_receipts_entry(session: AsyncSession, user_id: str, entry: CashReceiptsJournalEntryCreate, jwt_token: str) -> CashReceiptsJournalEntryInDB:
    """Create Cash Receipts Journal Entry and auto-generate journal entry"""
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (cr:CashReceiptsJournalEntry {
        id: $id,
        receipt_number: $receipt_number,
        customer_id: $customer_id,
        receipt_date: datetime($receipt_date),
        amount: toFloat($amount),
        payment_method: $payment_method,
        reference_number: $reference_number,
        bank_account: $bank_account,
        description: $description,
        source_type: $source_type,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_CASH_RECEIPTS_JOURNAL_ENTRY]->(cr)
    RETURN cr
    """

    params = {
        "id": entry_id, "user_id": user_id,
        "receipt_number": entry.receipt_number, "customer_id": entry.customer_id,
        "receipt_date": entry.receipt_date.isoformat(),
        "amount": float(entry.amount), "payment_method": entry.payment_method,
        "reference_number": entry.reference_number, "bank_account": entry.bank_account,
        "description": entry.description, "source_type": entry.source_type, "status": entry.status,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }

    # Auto-generate journal entry
    cash_account = entry.bank_account
    revenue_account = "4000" if entry.source_type == "cash_sale" else "4100"

    journal_lines = [
        JournalLineBase(account_number=cash_account, debit=entry.amount, credit=Decimal('0.00'), description=entry.description),
        JournalLineBase(account_number=revenue_account, debit=Decimal('0.00'), credit=entry.amount, description=entry.description)
    ]

    journal_entry_data = JournalEntryCreate(
        entry_date=entry.receipt_date,
        description=f"Cash Receipts: {entry.receipt_number}",
        reference_number=entry.receipt_number,
        source_module="CashReceiptsJournal",
        lines=journal_lines,
        status="posted"
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token)
    params["journal_entry_id"] = created_je.id

    query += """
    MATCH (cr_match:CashReceiptsJournalEntry {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (cr_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    """

    result = await session.run(query, params)
    record = await result.single()
    cr_node = record["cr"]

    return CashReceiptsJournalEntryInDB(
        id=cr_node["id"], user_id=user_id,
        receipt_number=cr_node["receipt_number"], customer_id=cr_node["customer_id"],
        receipt_date=datetime.fromisoformat(cr_node["receipt_date"].iso_format()),
        amount=Decimal(str(cr_node["amount"])),
        payment_method=cr_node["payment_method"], reference_number=cr_node["reference_number"],
        bank_account=cr_node["bank_account"], description=cr_node["description"],
        source_type=cr_node["source_type"], status=cr_node["status"],
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(cr_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(cr_node["updated_at"].iso_format())
    )

async def get_cash_receipts_entries(session: AsyncSession, user_id: str, start_date, end_date) -> List[CashReceiptsJournalEntryInDB]:
    date_filter = ""
    params = {"user_id": user_id}
    if start_date:
        date_filter += " AND cr.receipt_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND cr.receipt_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_RECEIPTS_JOURNAL_ENTRY]->(cr:CashReceiptsJournalEntry)
    WHERE true {date_filter}
    OPTIONAL MATCH (cr)-[:GENERATED_JOURNAL_ENTRY]->(je:JournalEntry)
    RETURN cr, je.id as journal_entry_id
    ORDER BY cr.receipt_date DESC
    """
    result = await session.run(query, params)
    entries = []
    async for record in result:
        cr = record["cr"]
        entries.append(CashReceiptsJournalEntryInDB(
            id=cr["id"], user_id=user_id,
            receipt_number=cr["receipt_number"], customer_id=cr["customer_id"],
            receipt_date=datetime.fromisoformat(cr["receipt_date"].iso_format()),
            amount=Decimal(str(cr["amount"])),
            payment_method=cr["payment_method"], reference_number=cr["reference_number"],
            bank_account=cr["bank_account"], description=cr["description"],
            source_type=cr["source_type"], status=cr["status"],
            journal_entry_id=record["journal_entry_id"],
            created_at=datetime.fromisoformat(cr["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(cr["updated_at"].iso_format())
        ))
    return entries

async def create_cash_disbursements_entry(session: AsyncSession, user_id: str, entry: CashDisbursementsJournalEntryCreate, jwt_token: str) -> CashDisbursementsJournalEntryInDB:
    """Create Cash Disbursements Journal Entry and auto-generate journal entry"""
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (cd:CashDisbursementsJournalEntry {
        id: $id,
        payment_number: $payment_number,
        vendor_id: $vendor_id,
        employee_id: $employee_id,
        payment_date: datetime($payment_date),
        amount: toFloat($amount),
        payment_method: $payment_method,
        reference_number: $reference_number,
        bank_account: $bank_account,
        description: $description,
        expense_account: $expense_account,
        source_type: $source_type,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_CASH_DISBURSEMENTS_JOURNAL_ENTRY]->(cd)
    RETURN cd
    """

    params = {
        "id": entry_id, "user_id": user_id,
        "payment_number": entry.payment_number, "vendor_id": entry.vendor_id,
        "employee_id": entry.employee_id, "payment_date": entry.payment_date.isoformat(),
        "amount": float(entry.amount), "payment_method": entry.payment_method,
        "reference_number": entry.reference_number, "bank_account": entry.bank_account,
        "description": entry.description, "expense_account": entry.expense_account,
        "source_type": entry.source_type, "status": entry.status,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }

    # Auto-generate journal entry
    cash_account = entry.bank_account
    expense_account = entry.expense_account

    journal_lines = [
        JournalLineBase(account_number=expense_account, debit=entry.amount, credit=Decimal('0.00'), description=entry.description),
        JournalLineBase(account_number=cash_account, debit=Decimal('0.00'), credit=entry.amount, description=entry.description)
    ]

    journal_entry_data = JournalEntryCreate(
        entry_date=entry.payment_date,
        description=f"Cash Disbursement: {entry.payment_number}",
        reference_number=entry.payment_number,
        source_module="CashDisbursementsJournal",
        lines=journal_lines,
        status="posted"
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token)
    params["journal_entry_id"] = created_je.id

    query += """
    MATCH (cd_match:CashDisbursementsJournalEntry {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (cd_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    """

    result = await session.run(query, params)
    record = await result.single()
    cd_node = record["cd"]

    return CashDisbursementsJournalEntryInDB(
        id=cd_node["id"], user_id=user_id,
        payment_number=cd_node["payment_number"], vendor_id=cd_node["vendor_id"],
        employee_id=cd_node["employee_id"], payment_date=datetime.fromisoformat(cd_node["payment_date"].iso_format()),
        amount=Decimal(str(cd_node["amount"])),
        payment_method=cd_node["payment_method"], reference_number=cd_node["reference_number"],
        bank_account=cd_node["bank_account"], description=cd_node["description"],
        expense_account=cd_node["expense_account"], source_type=cd_node["source_type"], status=cd_node["status"],
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(cd_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(cd_node["updated_at"].iso_format())
    )

async def get_cash_disbursements_entries(session: AsyncSession, user_id: str, start_date, end_date) -> List[CashDisbursementsJournalEntryInDB]:
    date_filter = ""
    params = {"user_id": user_id}
    if start_date:
        date_filter += " AND cd.payment_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND cd.payment_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CASH_DISBURSEMENTS_JOURNAL_ENTRY]->(cd:CashDisbursementsJournalEntry)
    WHERE true {date_filter}
    OPTIONAL MATCH (cd)-[:GENERATED_JOURNAL_ENTRY]->(je:JournalEntry)
    RETURN cd, je.id as journal_entry_id
    ORDER BY cd.payment_date DESC
    """
    result = await session.run(query, params)
    entries = []
    async for record in result:
        cd = record["cd"]
        entries.append(CashDisbursementsJournalEntryInDB(
            id=cd["id"], user_id=user_id,
            payment_number=cd["payment_number"], vendor_id=cd["vendor_id"],
            employee_id=cd["employee_id"], payment_date=datetime.fromisoformat(cd["payment_date"].iso_format()),
            amount=Decimal(str(cd["amount"])),
            payment_method=cd["payment_method"], reference_number=cd["reference_number"],
            bank_account=cd["bank_account"], description=cd["description"],
            expense_account=cd["expense_account"], source_type=cd["source_type"], status=cd["status"],
            journal_entry_id=record["journal_entry_id"],
            created_at=datetime.fromisoformat(cd["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(cd["updated_at"].iso_format())
        ))
    return entries

async def create_sales_return_entry(session: AsyncSession, user_id: str, entry: SalesReturnsJournalEntryCreate, jwt_token: str) -> SalesReturnsJournalEntryInDB:
    """Create Sales Returns Journal Entry and auto-generate journal entry"""
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (sr:SalesReturnsJournalEntry {
        id: $id,
        return_number: $return_number,
        original_invoice_number: $original_invoice_number,
        customer_id: $customer_id,
        return_date: datetime($return_date),
        total_amount: toFloat($total_amount),
        tax_amount: toFloat($tax_amount),
        reason: $reason,
        status: $status,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_SALES_RETURNS_JOURNAL_ENTRY]->(sr)
    RETURN sr
    """

    params = {
        "id": entry_id, "user_id": user_id,
        "return_number": entry.return_number, "original_invoice_number": entry.original_invoice_number,
        "customer_id": entry.customer_id, "return_date": entry.return_date.isoformat(),
        "total_amount": float(entry.total_amount), "tax_amount": float(entry.tax_amount),
        "reason": entry.reason, "status": entry.status, "notes": entry.notes,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }

    # Auto-generate journal entry (reverse the original sale)
    net_amount = entry.total_amount - entry.tax_amount
    sales_returns_account = "4100"  # Sales Returns
    accounts_receivable_account = "1100"  # Accounts Receivable

    journal_lines = [
        JournalLineBase(account_number=sales_returns_account, debit=net_amount, credit=Decimal('0.00'), description=f"Sales Return {entry.return_number}"),
        JournalLineBase(account_number=accounts_receivable_account, debit=Decimal('0.00'), credit=net_amount, description=f"Sales Return {entry.return_number}")
    ]

    journal_entry_data = JournalEntryCreate(
        entry_date=entry.return_date,
        description=f"Sales Returns: {entry.return_number}",
        reference_number=entry.return_number,
        source_module="SalesReturnsJournal",
        lines=journal_lines,
        status="posted"
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token)
    params["journal_entry_id"] = created_je.id

    query += """
    MATCH (sr_match:SalesReturnsJournalEntry {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (sr_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    """

    result = await session.run(query, params)
    record = await result.single()
    sr_node = record["sr"]

    return SalesReturnsJournalEntryInDB(
        id=sr_node["id"], user_id=user_id,
        return_number=sr_node["return_number"], original_invoice_number=sr_node["original_invoice_number"],
        customer_id=sr_node["customer_id"], return_date=datetime.fromisoformat(sr_node["return_date"].iso_format()),
        total_amount=Decimal(str(sr_node["total_amount"])),
        tax_amount=Decimal(str(sr_node["tax_amount"])),
        reason=sr_node["reason"], status=sr_node["status"], notes=sr_node["notes"],
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(sr_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(sr_node["updated_at"].iso_format())
    )

async def create_purchases_return_entry(session: AsyncSession, user_id: str, entry: PurchasesReturnsJournalEntryCreate, jwt_token: str) -> PurchasesReturnsJournalEntryInDB:
    """Create Purchases Returns Journal Entry and auto-generate journal entry"""
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (pr:PurchasesReturnsJournalEntry {
        id: $id,
        return_number: $return_number,
        original_po_number: $original_po_number,
        vendor_id: $vendor_id,
        return_date: datetime($return_date),
        total_amount: toFloat($total_amount),
        tax_amount: toFloat($tax_amount),
        reason: $reason,
        status: $status,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_PURCHASES_RETURNS_JOURNAL_ENTRY]->(pr)
    RETURN pr
    """

    params = {
        "id": entry_id, "user_id": user_id,
        "return_number": entry.return_number, "original_po_number": entry.original_po_number,
        "vendor_id": entry.vendor_id, "return_date": entry.return_date.isoformat(),
        "total_amount": float(entry.total_amount), "tax_amount": float(entry.tax_amount),
        "reason": entry.reason, "status": entry.status, "notes": entry.notes,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }

    # Auto-generate journal entry (reverse the original purchase)
    net_amount = entry.total_amount - entry.tax_amount
    purchases_returns_account = "5100"  # Purchases Returns
    accounts_payable_account = "2000"  # Accounts Payable

    journal_lines = [
        JournalLineBase(account_number=accounts_payable_account, debit=net_amount, credit=Decimal('0.00'), description=f"Purchases Return {entry.return_number}"),
        JournalLineBase(account_number=purchases_returns_account, debit=Decimal('0.00'), credit=net_amount, description=f"Purchases Return {entry.return_number}")
    ]

    journal_entry_data = JournalEntryCreate(
        entry_date=entry.return_date,
        description=f"Purchases Returns: {entry.return_number}",
        reference_number=entry.return_number,
        source_module="PurchasesReturnsJournal",
        lines=journal_lines,
        status="posted"
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token)
    params["journal_entry_id"] = created_je.id

    query += """
    MATCH (pr_match:PurchasesReturnsJournalEntry {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (pr_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    """

    result = await session.run(query, params)
    record = await result.single()
    pr_node = record["pr"]

    return PurchasesReturnsJournalEntryInDB(
        id=pr_node["id"], user_id=user_id,
        return_number=pr_node["return_number"], original_po_number=pr_node["original_po_number"],
        vendor_id=pr_node["vendor_id"], return_date=datetime.fromisoformat(pr_node["return_date"].iso_format()),
        total_amount=Decimal(str(pr_node["total_amount"])),
        tax_amount=Decimal(str(pr_node["tax_amount"])),
        reason=pr_node["reason"], status=pr_node["status"], notes=pr_node["notes"],
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(pr_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(pr_node["updated_at"].iso_format())
    )


# ============================================================
# SUBSIDIARY LEDGERS CRUD
# ============================================================

async def get_ar_ledger_report(session: AsyncSession, user_id: str, as_of_date, customer_id) -> AccountsReceivableLedgerReport:
    """Get Accounts Receivable Subsidiary Ledger"""
    date_filter = ""
    params = {"user_id": user_id}
    if as_of_date:
        date_filter += " AND je.entry_date <= datetime($as_of_date)"
        params["as_of_date"] = as_of_date.isoformat()
    if customer_id:
        date_filter += " AND je.customer_id = $customer_id"
        params["customer_id"] = customer_id

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SALES_JOURNAL_ENTRY]->(sj:SalesJournalEntry)
    WHERE true {date_filter}
    RETURN sj
    ORDER BY sj.invoice_date DESC
    """
    result = await session.run(query, params)
    entries = []
    total_invoice = Decimal('0.00')
    total_balance = Decimal('0.00')
    total_paid = Decimal('0.00')
    overdue_count = 0

    async for record in result:
        sj = record["sj"]
        invoice_date = datetime.fromisoformat(sj["invoice_date"].iso_format())
        days_outstanding = (datetime.utcnow() - invoice_date).days
        total_amount = Decimal(str(sj["total_amount"]))
        balance_due = total_amount  # Simplified - would need payments tracking
        status = "overdue" if days_outstanding > 30 else sj["status"]

        if status == "overdue":
            overdue_count += 1

        entries.append(AccountsReceivableLedgerEntry(
            customer_id=sj["customer_id"],
            customer_name=f"Customer {sj['customer_id'][:8]}",  # Placeholder
            invoice_number=sj["invoice_number"],
            invoice_date=invoice_date,
            due_date=datetime.fromisoformat(sj["due_date"].iso_format()) if sj["due_date"] else invoice_date,
            invoice_amount=total_amount,
            balance_due=balance_due,
            amount_paid=Decimal('0.00'),
            status=status,
            days_outstanding=days_outstanding
        ))
        total_invoice += total_amount
        total_balance += balance_due

    return AccountsReceivableLedgerReport(
        as_of_date=as_of_date if as_of_date else datetime.utcnow(),
        entries=entries,
        total_invoice_amount=total_invoice,
        total_balance_due=total_balance,
        total_amount_paid=total_paid,
        customer_count=len(set(e.customer_id for e in entries)),
        overdue_count=overdue_count
    )

async def get_ap_ledger_report(session: AsyncSession, user_id: str, as_of_date, vendor_id) -> AccountsPayableLedgerReport:
    """Get Accounts Payable Subsidiary Ledger"""
    date_filter = ""
    params = {"user_id": user_id}
    if as_of_date:
        date_filter += " AND je.entry_date <= datetime($as_of_date)"
        params["as_of_date"] = as_of_date.isoformat()
    if vendor_id:
        date_filter += " AND je.vendor_id = $vendor_id"
        params["vendor_id"] = vendor_id

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PURCHASES_JOURNAL_ENTRY]->(pj:PurchasesJournalEntry)
    WHERE true {date_filter}
    RETURN pj
    ORDER BY pj.purchase_date DESC
    """
    result = await session.run(query, params)
    entries = []
    total_bill = Decimal('0.00')
    total_balance = Decimal('0.00')
    total_paid = Decimal('0.00')
    overdue_count = 0

    async for record in result:
        pj = record["pj"]
        bill_date = datetime.fromisoformat(pj["purchase_date"].iso_format())
        days_outstanding = (datetime.utcnow() - bill_date).days
        total_amount = Decimal(str(pj["total_amount"]))
        balance_due = total_amount
        status = "overdue" if days_outstanding > 30 else pj["status"]

        if status == "overdue":
            overdue_count += 1

        entries.append(AccountsPayableLedgerEntry(
            vendor_id=pj["vendor_id"],
            vendor_name=f"Vendor {pj['vendor_id'][:8]}",
            bill_number=pj["purchase_order_number"],
            bill_date=bill_date,
            due_date=datetime.fromisoformat(pj["due_date"].iso_format()) if pj["due_date"] else bill_date,
            bill_amount=total_amount,
            balance_due=balance_due,
            amount_paid=Decimal('0.00'),
            status=status,
            days_outstanding=days_outstanding
        ))
        total_bill += total_amount
        total_balance += balance_due

    return AccountsPayableLedgerReport(
        as_of_date=as_of_date if as_of_date else datetime.utcnow(),
        entries=entries,
        total_bill_amount=total_bill,
        total_balance_due=total_balance,
        total_amount_paid=total_paid,
        vendor_count=len(set(e.vendor_id for e in entries)),
        overdue_count=overdue_count
    )

async def get_fixed_assets_ledger(session: AsyncSession, user_id: str, as_of_date, status) -> FixedAssetsLedgerReport:
    """Get Fixed Assets Subsidiary Ledger"""
    # Placeholder implementation - would integrate with asset management
    return FixedAssetsLedgerReport(
        as_of_date=as_of_date if as_of_date else datetime.utcnow(),
        entries=[],
        total_purchase_cost=Decimal('0.00'),
        total_accumulated_depreciation=Decimal('0.00'),
        total_net_book_value=Decimal('0.00'),
        asset_count=0
    )

async def get_inventory_ledger(session: AsyncSession, user_id: str, as_of_date, category) -> InventoryLedgerReport:
    """Get Inventory Subsidiary Ledger"""
    # Placeholder - would integrate with supply chain service
    return InventoryLedgerReport(
        as_of_date=as_of_date if as_of_date else datetime.utcnow(),
        entries=[],
        total_opening_value=Decimal('0.00'),
        total_stock_in_value=Decimal('0.00'),
        total_stock_out_value=Decimal('0.00'),
        total_closing_value=Decimal('0.00'),
        item_count=0,
        low_stock_count=0
    )


# ============================================================
# PETTY CASH CRUD
# ============================================================

async def create_petty_cash_fund(session: AsyncSession, user_id: str, fund: PettyCashFundCreate) -> PettyCashFundInDB:
    """Create Petty Cash Fund"""
    fund_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (pcf:PettyCashFund {
        id: $id,
        fund_name: $fund_name,
        fund_number: $fund_number,
        imprest_amount: toFloat($imprest_amount),
        current_balance: toFloat($current_balance),
        custodian: $custodian,
        location: $location,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_PETTY_CASH_FUND]->(pcf)
    RETURN pcf
    """
    params = {
        "id": fund_id, "user_id": user_id,
        "fund_name": fund.fund_name, "fund_number": fund.fund_number,
        "imprest_amount": float(fund.imprest_amount), "current_balance": float(fund.current_balance),
        "custodian": fund.custodian, "location": fund.location,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }
    result = await session.run(query, params)
    record = await result.single()
    pcf = record["pcf"]

    return PettyCashFundInDB(
        id=pcf["id"], user_id=user_id,
        fund_name=pcf["fund_name"], fund_number=pcf["fund_number"],
        imprest_amount=Decimal(str(pcf["imprest_amount"])),
        current_balance=Decimal(str(pcf["current_balance"])),
        custodian=pcf["custodian"], location=pcf["location"],
        created_at=datetime.fromisoformat(pcf["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(pcf["updated_at"].iso_format())
    )

async def get_petty_cash_funds(session: AsyncSession, user_id: str) -> List[PettyCashFundInDB]:
    """Get all Petty Cash Funds"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_PETTY_CASH_FUND]->(pcf:PettyCashFund)
    RETURN pcf
    ORDER BY pcf.fund_number
    """
    result = await session.run(query, user_id=user_id)
    funds = []
    async for record in result:
        pcf = record["pcf"]
        funds.append(PettyCashFundInDB(
            id=pcf["id"], user_id=user_id,
            fund_name=pcf["fund_name"], fund_number=pcf["fund_number"],
            imprest_amount=Decimal(str(pcf["imprest_amount"])),
            current_balance=Decimal(str(pcf["current_balance"])),
            custodian=pcf["custodian"], location=pcf["location"],
            created_at=datetime.fromisoformat(pcf["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(pcf["updated_at"].iso_format())
        ))
    return funds

async def create_petty_cash_entry(session: AsyncSession, user_id: str, entry: PettyCashEntryCreate, jwt_token: str) -> PettyCashEntryInDB:
    """Create Petty Cash Entry and auto-generate journal entry"""
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id}), (pcf:PettyCashFund {id: $petty_cash_fund_id})
    CREATE (pce:PettyCashEntry {
        id: $id,
        voucher_number: $voucher_number,
        voucher_date: datetime($voucher_date),
        payee: $payee,
        amount: toFloat($amount),
        category: $category,
        description: $description,
        receipt_number: $receipt_number,
        approved_by: $approved_by,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_PETTY_CASH_ENTRY]->(pce)
    CREATE (pce)-[:FROM_FUND]->(pcf)
    RETURN pce, pcf
    """
    params = {
        "id": entry_id, "user_id": user_id,
        "petty_cash_fund_id": entry.petty_cash_fund_id,
        "voucher_number": entry.voucher_number, "voucher_date": entry.voucher_date.isoformat(),
        "payee": entry.payee, "amount": float(entry.amount),
        "category": entry.category, "description": entry.description,
        "receipt_number": entry.receipt_number, "approved_by": entry.approved_by,
        "created_at": created_at.isoformat(), "updated_at": created_at.isoformat()
    }

    # Auto-generate journal entry
    petty_cash_account = "1001"  # Petty Cash
    expense_mapping = {
        'office_supplies': '6100', 'postage': '6200', 'transportation': '6300',
        'meals': '6400', 'tips': '6500', 'miscellaneous': '6600'
    }
    expense_account = expense_mapping.get(entry.category, '6600')

    journal_lines = [
        JournalLineBase(account_number=expense_account, debit=entry.amount, credit=Decimal('0.00'), description=f"Petty Cash: {entry.voucher_number}"),
        JournalLineBase(account_number=petty_cash_account, debit=Decimal('0.00'), credit=entry.amount, description=f"Petty Cash: {entry.voucher_number}")
    ]

    journal_entry_data = JournalEntryCreate(
        entry_date=entry.voucher_date,
        description=f"Petty Cash Voucher: {entry.voucher_number}",
        reference_number=entry.voucher_number,
        source_module="PettyCash",
        lines=journal_lines,
        status="posted"
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_data, jwt_token)
    params["journal_entry_id"] = created_je.id

    query += """
    MATCH (pce_match:PettyCashEntry {id: $id}), (je_match:JournalEntry {id: $journal_entry_id})
    CREATE (pce_match)-[:GENERATED_JOURNAL_ENTRY]->(je_match)
    """

    result = await session.run(query, params)
    record = await result.single()
    pce = record["pce"]

    return PettyCashEntryInDB(
        id=pce["id"], user_id=user_id, petty_cash_fund_id=entry.petty_cash_fund_id,
        voucher_number=pce["voucher_number"],
        voucher_date=datetime.fromisoformat(pce["voucher_date"].iso_format()),
        payee=pce["payee"], amount=Decimal(str(pce["amount"])),
        category=pce["category"], description=pce["description"],
        receipt_number=pce["receipt_number"], approved_by=pce["approved_by"],
        journal_entry_id=created_je.id,
        created_at=datetime.fromisoformat(pce["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(pce["updated_at"].iso_format())
    )

async def get_petty_cash_entries(session: AsyncSession, user_id: str, fund_id, start_date, end_date) -> List[PettyCashEntryInDB]:
    """Get Petty Cash Entries"""
    date_filter = ""
    params = {"user_id": user_id}
    if fund_id:
        date_filter += " AND pce.petty_cash_fund_id = $fund_id"
        params["fund_id"] = fund_id
    if start_date:
        date_filter += " AND pce.voucher_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND pce.voucher_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PETTY_CASH_ENTRY]->(pce:PettyCashEntry)-[:FROM_FUND]->(pcf:PettyCashFund)
    WHERE true {date_filter}
    OPTIONAL MATCH (pce)-[:GENERATED_JOURNAL_ENTRY]->(je:JournalEntry)
    RETURN pce, pcf.id as petty_cash_fund_id, je.id as journal_entry_id
    ORDER BY pce.voucher_date DESC
    """
    result = await session.run(query, params)
    entries = []
    async for record in result:
        pce = record["pce"]
        entries.append(PettyCashEntryInDB(
            id=pce["id"], user_id=user_id,
            petty_cash_fund_id=record["petty_cash_fund_id"],
            voucher_number=pce["voucher_number"],
            voucher_date=datetime.fromisoformat(pce["voucher_date"].iso_format()),
            payee=pce["payee"], amount=Decimal(str(pce["amount"])),
            category=pce["category"], description=pce["description"],
            receipt_number=pce["receipt_number"], approved_by=pce["approved_by"],
            journal_entry_id=record["journal_entry_id"],
            created_at=datetime.fromisoformat(pce["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(pce["updated_at"].iso_format())
        ))
    return entries


# ============================================================
# BANK RECONCILIATION CRUD
# ============================================================

async def create_bank_reconciliation(session: AsyncSession, user_id: str, bank_account: str, statement_date: datetime, statement_balance: Decimal, jwt_token: str) -> BankReconciliationStatement:
    """Create Bank Reconciliation Statement"""
    recon_id = str(uuid.uuid4())
    created_at = datetime.utcnow()

    # Get bank account transactions from journal entries
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {account_number: $bank_account})
    WHERE je.status = 'posted' AND je.entry_date <= datetime($statement_date)
    RETURN je, jl
    ORDER BY je.entry_date ASC
    """
    result = await session.run(query, user_id=user_id, bank_account=bank_account, statement_date=statement_date.isoformat())

    bank_entries = []
    journal_entries = []
    total_debits = Decimal('0.00')
    total_credits = Decimal('0.00')

    async for record in result:
        je = record["je"]
        jl = record["jl"]
        debit = Decimal(str(jl["debit"]))
        credit = Decimal(str(jl["credit"]))
        running = total_debits - total_credits if debit > 0 else total_credits - total_debits

        entry = BankReconciliationEntry(
            transaction_date=datetime.fromisoformat(je["entry_date"].iso_format()),
            description=je["description"],
            debit=debit,
            credit=credit,
            running_balance=running,
            reference=je["reference_number"],
            matched=True,
            match_type="journal_entry",
            matched_entry_id=je["id"]
        )
        journal_entries.append(entry)
        total_debits += debit
        total_credits += credit

    book_balance = total_debits - total_credits

    # Calculate adjustments
    deposits_in_transit = []
    outstanding_checks = []
    bank_charges = Decimal('0.00')
    interest_earned = Decimal('0.00')
    insufficient_funds = Decimal('0.00')

    adjusted_bank = statement_balance - sum(e.credit for e in deposits_in_transit) + sum(e.debit for e in outstanding_checks)
    adjusted_book = book_balance - bank_charges + interest_earned - insufficient_funds
    difference = adjusted_bank - adjusted_book

    reconciliation = BankReconciliationStatement(
        bank_account_number=bank_account,
        bank_name=f"Bank Account {bank_account}",
        statement_date=statement_date,
        statement_balance=statement_balance,
        book_balance=book_balance,
        deposits_in_transit=deposits_in_transit,
        outstanding_checks=outstanding_checks,
        bank_charges=bank_charges,
        interest_earned=interest_earned,
        insufficient_funds=insufficient_funds,
        adjusted_bank_balance=adjusted_bank,
        adjusted_book_balance=adjusted_book,
        difference=difference,
        bank_entries=bank_entries,
        journal_entries=journal_entries,
        is_reconciled=difference == Decimal('0.00'),
        reconciled_date=None,
        reconciled_by=None
    )

    # Store in Neo4j
    store_query = """
    MATCH (u:User {id: $user_id})
    CREATE (br:BankReconciliation {
        id: $id,
        bank_account_number: $bank_account,
        statement_date: datetime($statement_date),
        statement_balance: toFloat($statement_balance),
        book_balance: toFloat($book_balance),
        adjusted_bank_balance: toFloat($adjusted_bank_balance),
        adjusted_book_balance: toFloat($adjusted_book_balance),
        difference: toFloat($difference),
        is_reconciled: $is_reconciled,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_BANK_RECONCILIATION]->(br)
    RETURN br
    """
    store_params = {
        "id": recon_id, "user_id": user_id,
        "bank_account": bank_account, "statement_date": statement_date.isoformat(),
        "statement_balance": float(statement_balance), "book_balance": float(book_balance),
        "adjusted_bank_balance": float(adjusted_bank), "adjusted_book_balance": float(adjusted_book),
        "difference": float(difference), "is_reconciled": difference == Decimal('0.00'),
        "created_at": created_at.isoformat()
    }
    await session.run(store_query, store_params)

    return reconciliation

async def get_bank_reconciliations(session: AsyncSession, user_id: str, bank_account, start_date, end_date) -> List[BankReconciliationStatement]:
    """Get Bank Reconciliations"""
    date_filter = ""
    params = {"user_id": user_id}
    if bank_account:
        date_filter += " AND br.bank_account_number = $bank_account"
        params["bank_account"] = bank_account
    if start_date:
        date_filter += " AND br.statement_date >= datetime($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND br.statement_date <= datetime($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_RECONCILIATION]->(br:BankReconciliation)
    WHERE true {date_filter}
    RETURN br
    ORDER BY br.statement_date DESC
    """
    result = await session.run(query, params)
    reconciliations = []
    async for record in result:
        br = record["br"]
        reconciliations.append(BankReconciliationStatement(
            bank_account_number=br["bank_account_number"],
            bank_name=f"Bank Account {br['bank_account_number']}",
            statement_date=datetime.fromisoformat(br["statement_date"].iso_format()),
            statement_balance=Decimal(str(br["statement_balance"])),
            book_balance=Decimal(str(br["book_balance"])),
            deposits_in_transit=[],
            outstanding_checks=[],
            bank_charges=Decimal('0.00'),
            interest_earned=Decimal('0.00'),
            insufficient_funds=Decimal('0.00'),
            adjusted_bank_balance=Decimal(str(br["adjusted_bank_balance"])),
            adjusted_book_balance=Decimal(str(br["adjusted_book_balance"])),
            difference=Decimal(str(br["difference"])),
            bank_entries=[],
            journal_entries=[],
            is_reconciled=br["is_reconciled"],
            reconciled_date=None,
            reconciled_by=None
        ))
    return reconciliations

async def get_latest_bank_reconciliation(session: AsyncSession, user_id: str, bank_account: str) -> BankReconciliationStatement:
    """Get Latest Bank Reconciliation for an account"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BANK_RECONCILIATION]->(br:BankReconciliation {bank_account_number: $bank_account})
    RETURN br
    ORDER BY br.statement_date DESC
    LIMIT 1
    """
    result = await session.run(query, user_id=user_id, bank_account=bank_account)
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"No bank reconciliation found for account {bank_account}")
    br = record["br"]
    return BankReconciliationStatement(
        bank_account_number=br["bank_account_number"],
        bank_name=f"Bank Account {br['bank_account_number']}",
        statement_date=datetime.fromisoformat(br["statement_date"].iso_format()),
        statement_balance=Decimal(str(br["statement_balance"])),
        book_balance=Decimal(str(br["book_balance"])),
        deposits_in_transit=[],
        outstanding_checks=[],
        bank_charges=Decimal('0.00'),
        interest_earned=Decimal('0.00'),
        insufficient_funds=Decimal('0.00'),
        adjusted_bank_balance=Decimal(str(br["adjusted_bank_balance"])),
        adjusted_book_balance=Decimal(str(br["adjusted_book_balance"])),
        difference=Decimal(str(br["difference"])),
        bank_entries=[],
        journal_entries=[],
        is_reconciled=br["is_reconciled"],
        reconciled_date=None,
        reconciled_by=None
    )
