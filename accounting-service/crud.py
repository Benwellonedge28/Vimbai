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

# --- Account CRUD (unchanged) ---
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
        return await get_account(session, user_id, account_number)

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

async def get_journal_entry_by_reference(session: AsyncSession, user_id: str, reference_number: str, source_module: str) -> Optional[JournalEntryInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry {reference_number: $reference_number, source_module: $source_module})
    RETURN je
    """
    result = await session.run(query, user_id=user_id, reference_number=reference_number, source_module=source_module)
    record = await result.single()
    if record:
        je_node = record["je"]
        # Note: This partial JE object might not have full lines, but only used for existence check
        return JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            lines=[], # Not fetching lines here for quick check
            fraud_flag=je_node["fraud_flag"],
            fraud_score=je_node["fraud_score"],
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
        )
    return None

# --- Helper to convert JournalEntry to TransactionForFraudCheck (NEW) ---
async def _send_journal_entry_for_fraud_analysis(user_id: str, journal_entry_data: JournalEntryCreate, jwt_token: str) -> FraudDetectionResult:
    """Internal helper to send journal entry data to Fraud Detection Service."""

    # Aggregate JE lines into a single pseudo-transaction for fraud analysis
    total_debit = sum(line.debit for line in journal_entry_data.lines)
    
    # For a balanced JE, total_debit == total_credits. Use one as the transaction amount.
    transaction_amount = total_debit 

    # Infer sender/recipient from common JE patterns or use placeholders
    sender_account = ""
    recipient_account = ""
    
    # Example: if a JE debits an expense and credits cash, cash is the source, expense is the destination
    # For simplicity, we can use the first debit and credit accounts.
    debit_accounts = [line.account_number for line in journal_entry_data.lines if line.debit > 0]
    credit_accounts = [line.account_number for line in journal_entry_data.lines if line.credit > 0]

    if debit_accounts: sender_account = debit_accounts[0] # Often the 'source' of the value
    if credit_accounts: recipient_account = credit_accounts[0] # Often the 'destination' of the value

    transaction_type = "journal_entry" # Custom type for fraud service
    
    transaction_for_fraud = TransactionForFraudCheck(
        transaction_id=journal_entry_data.reference_number or str(uuid.uuid4()), # Use reference_number as transaction_id
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

# --- Vendor Bill Management (NEW) ---
async def create_vendor_bill_from_po(session: AsyncSession, user_id: str, vendor_bill_data: VendorBillCreate, jwt_token: str) -> VendorBillInDB:
    # 1. Fetch Purchase Order details from Supply Chain Service
    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    async with httpx.AsyncClient() as client:
        try:
            po_response = await client.get(
                f"{API_GATEWAY_URL}/purchase-orders/{vendor_bill_data.purchase_order_id}",
                headers=headers
            )
            po_response.raise_for_status()
            po_details = po_response.json()
            purchase_order = PurchaseOrderInDB(**po_details)
        except httpx.HTTPStatusError as e:
            error_detail = e.response.json().get("detail", e.response.text)
            raise NotFoundError(detail=f"Purchase Order not found or inaccessible: {error_detail}", code="PURCHASE_ORDER_NOT_FOUND")
        except httpx.RequestError as e:
            raise ValidationError(detail=f"Network error communicating with Supply Chain Service: {e}", code="UPSTREAM_SC_NETWORK_ERROR")

    # 2. Construct Journal Entry lines
    # Debit Inventory/Expense for each item, Credit Accounts Payable for total
    journal_lines: List[JournalLineBase] = []

    # For each item in the PO, debit an Inventory Asset or Expense account.
    # For simplicity, we'll use a generic Inventory Asset account (e.g., 1400) and Expense (6000).
    # In a real system, this mapping would be more sophisticated (e.g., based on item type).
    inventory_asset_account = "1400" # Example Inventory Asset Account
    expense_account = "6000" # Example Expense Account (for services or non-inventory purchases)
    accounts_payable_account = "2000" # Example Accounts Payable Account

    for item in purchase_order.items:
        # Decide if it's an asset (inventory) or expense. Simplified: assume all are inventory for now.
        # Debit the Inventory Asset account
        journal_lines.append(JournalLineBase(
            account_number=inventory_asset_account, 
            debit=item.line_total, 
            credit=Decimal('0.00'), 
            description=f"Purchase of {item.quantity} units of Inventory Item {item.inventory_item_id}"
        ))

    # Add any additional lines specified in the VendorBillCreate request
    if vendor_bill_data.additional_lines:
        journal_lines.extend(vendor_bill_data.additional_lines)

    # Calculate total amount to credit to Accounts Payable (PO total + additional lines total)
    total_bill_amount = purchase_order.total_amount + sum(line.debit - line.credit for line in vendor_bill_data.additional_lines or [])
    journal_lines.append(JournalLineBase(
        account_number=accounts_payable_account,
        debit=Decimal('0.00'),
        credit=total_bill_amount,
        description=f"Vendor Bill for PO {purchase_order.id}"
    ))

    # 3. Create Journal Entry
    journal_entry_create_data = JournalEntryCreate(
        entry_date=vendor_bill_data.bill_date,
        description=f"Vendor Bill for Purchase Order {purchase_order.id} from {purchase_order.supplier_id}",
        reference_number=f"VB-{purchase_order.id}",
        source_module="SupplyChain",
        lines=journal_lines,
        status="posted" # Vendor bills are typically posted directly
    )

    created_je = await create_journal_entry(session, user_id, journal_entry_create_data, jwt_token) # Reuse existing JE creation

    # 4. Create VendorBillInDB record (linking PO to JE)
    vendor_bill_id = str(uuid.uuid4())
    query_vb = """
    MATCH (u:User {id: $user_id})
    MATCH (po:PurchaseOrder {id: $purchase_order_id})
    MATCH (je:JournalEntry {id: $journal_entry_id})
    CREATE (vb:VendorBill {
        id: $id,
        purchase_order_id: $purchase_order_id,
        bill_date: datetime($bill_date),
        due_date: datetime($due_date),
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_VENDOR_BILL]->(vb)
    CREATE (vb)-[:RELATED_TO_PO]->(po)
    CREATE (vb)-[:CREATED_JE]->(je)
    RETURN vb
    """
    params_vb = {
        "id": vendor_bill_id,
        "user_id": user_id,
        "purchase_order_id": vendor_bill_data.purchase_order_id,
        "bill_date": vendor_bill_data.bill_date.isoformat(),
        "due_date": vendor_bill_data.due_date.isoformat(),
        "journal_entry_id": created_je.id,
        "created_at": datetime.utcnow().isoformat()
    }
    result_vb = await session.run(query_vb, params_vb)
    vb_node = (await result_vb.single())["vb"]

    return VendorBillInDB(
        id=vb_node["id"],
        purchase_order_id=vb_node["purchase_order_id"],
        bill_date=datetime.fromisoformat(vb_node["bill_date"].iso_format()),
        due_date=datetime.fromisoformat(vb_node["due_date"].iso_format()),
        journal_entry_id=vb_node["journal_entry_id"],
        created_at=datetime.fromisoformat(vb_node["created_at"].iso_format()),
    )


# --- Ledger Reports (unchanged) ---
async def get_ledger_report(session: AsyncSession, user_id: str, account_number: str, start_date: datetime, end_date: datetime) -> LedgerReport:
    # ... (unchanged) ...
    pass

# --- Trial Balance (unchanged) ---
async def get_trial_balance_report(session: AsyncSession, user_id: str, as_of_date: datetime) -> TrialBalanceReport:
    # ... (unchanged) ...
    pass

# --- Financial Statements (unchanged) ---
async def get_income_statement(session: AsyncSession, user_id: str, start_date: datetime, end_date: datetime) -> IncomeStatement:
    # ... (unchanged) ...
    pass

async def get_balance_sheet(session: AsyncSession, user_id: str, as_of_date: datetime) -> BalanceSheet:
    # ... (unchanged) ...
    pass
