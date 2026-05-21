from neo4j import AsyncSession
from typing import Optional, List, Dict, Any, Tuple
from accounting_service.models import (
    AccountCreate, AccountUpdate, AccountInDB,
    JournalEntryCreate, JournalEntryUpdate, JournalEntryInDB, JournalLineBase,
    LedgerReport, LedgerEntry, TrialBalanceReport, TrialBalanceAccount,
    IncomeStatement, BalanceSheet, FinancialStatementLine,
    TransactionForFraudCheck, FraudDetectionResult, # From Fraud Detection Service
    PurchaseOrderInDB, PurchaseOrderItemBase, # From Supply Chain Service
    VendorBillCreate, VendorBillInDB # NEW
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
