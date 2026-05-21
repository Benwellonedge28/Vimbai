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
    journal_entry_data.fraud_flag = fraud_result.fraud_flag
    journal_entry_data.fraud_score = fraud_result.fraud_score

    # Create JournalEntry node
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
    RETURN je
    """
    params = journal_entry_data.model_dump(exclude={"lines"})
    params["id"] = entry_neo4j_id
    params["user_id"] = user_id
    params["entry_date"] = journal_entry_data.entry_date.isoformat()
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    je_node = record["je"]

    # Create JournalLine nodes and link them
    for i, line_data in enumerate(journal_entry_data.lines):
        line_neo4j_id = str(uuid.uuid4())
        line_query = """
        MATCH (je:JournalEntry {id: $je_id})
        MATCH (a:Account {account_number: $account_number})
        CREATE (jl:JournalLine {
            id: $id,
            debit: $debit,
            credit: $credit,
            description: $description,
            created_at: datetime($created_at),
            updated_at: datetime($updated_at)
        })
        CREATE (je)-[:HAS_LINE {order: $order}]->(jl)
        CREATE (jl)-[:IMPACTS]->(a)
        RETURN jl
        """
        line_params = line_data.model_dump()
        line_params["id"] = line_neo4j_id
        line_params["je_id"] = entry_neo4j_id
        line_params["order"] = i
        line_params["created_at"] = created_at.isoformat()
        line_params["updated_at"] = updated_at.isoformat()

        await session.run(line_query, line_params)

    return JournalEntryInDB(
        id=je_node["id"],
        user_id=user_id,
        entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
        description=je_node["description"],
        reference_number=je_node["reference_number"],
        source_module=je_node["source_module"],
        status=je_node["status"],
        fraud_flag=je_node["fraud_flag"],
        fraud_score=je_node["fraud_score"],
        created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
        lines=journal_entry_data.lines
    )

async def get_journal_entry(session: AsyncSession, user_id: str, entry_id: str) -> Optional[JournalEntryInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry {id: $entry_id})
    OPTIONAL MATCH (je)-[hl:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account)
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

    if record and record["je"]:
        je_node = record["je"]
        lines_data = record["lines_data"]
        lines = [JournalLineBase(
            account_number=line["account_number"],
            debit=Decimal(str(line["debit"])),
            credit=Decimal(str(line["credit"])),
            description=line["description"]
        ) for line in lines_data]

        return JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            fraud_flag=je_node.get("fraud_flag", "safe"), # Default for older entries
            fraud_score=je_node.get("fraud_score"), # Default for older entries
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            lines=lines
        )
    return None

async def get_all_journal_entries(session: AsyncSession, user_id: str, start_date: Optional[datetime] = None, end_date: Optional[datetime] = None) -> List[JournalEntryInDB]:
    date_filter = ""
    if start_date:
        date_filter += " AND je.entry_date >= datetime($start_date)"
    if end_date:
        date_filter += " AND je.entry_date <= datetime($end_date)"

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)
    OPTIONAL MATCH (je)-[hl:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account)
    WHERE TRUE {date_filter}
    RETURN je, COLLECT({{
        id: jl.id,
        account_number: a.account_number,
        debit: jl.debit,
        credit: jl.credit,
        description: jl.description
    }}) AS lines_data
    ORDER BY je.entry_date DESC
    """
    params = {"user_id": user_id}
    if start_date:
        params["start_date"] = start_date.isoformat()
    if end_date:
        params["end_date"] = end_date.isoformat()

    result = await session.run(query, params)
    journal_entries = []
    async for record in result:
        je_node = record["je"]
        lines_data = record["lines_data"]
        lines = [JournalLineBase(
            account_number=line["account_number"],
            debit=Decimal(str(line["debit"])),
            credit=Decimal(str(line["credit"])),
            description=line["description"]
        ) for line in lines_data if line["account_number"] is not None] # Filter out null lines for entries without lines

        journal_entries.append(JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            fraud_flag=je_node.get("fraud_flag", "safe"),
            fraud_score=je_node.get("fraud_score"),
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            lines=lines
        ))
    return journal_entries

async def update_journal_entry(session: AsyncSession, user_id: str, entry_id: str, journal_entry_data: JournalEntryUpdate) -> Optional[JournalEntryInDB]:
    update_fields = journal_entry_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_journal_entry(session, user_id, entry_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()

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
    # Check if entry is already posted
    existing_je = await get_journal_entry(session, user_id, entry_id)
    if existing_je and existing_je.status == 'posted':
        raise ConflictError(detail="Posted journal entries cannot be deleted directly.", code="POSTED_JE_CANNOT_DELETE")

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
        return JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            lines=[] # Lines not fetched in this specific query
        )
    return None

async def _send_journal_entry_for_fraud_analysis(user_id: str, journal_entry_data: JournalEntryCreate, jwt_token: str) -> FraudDetectionResult:
    # This is a placeholder for actual inter-service communication
    # In a real scenario, this would call the Fraud Detection Service
    # using httpx or a message queue.

    # For now, simulate a response
    print(f"Simulating fraud analysis for JE: {journal_entry_data.description}")

    # Example: Simple heuristic (e.g., large amount = higher risk)
    total_amount = sum(line.debit for line in journal_entry_data.lines) # Debits == Credits

    if total_amount > 100000:
        fraud_flag = "high_risk"
        fraud_score = 0.95
        reason = "Very high transaction amount detected."
    elif total_amount > 50000:
        fraud_flag = "suspicious"
        fraud_score = 0.7
        reason = "High transaction amount detected."
    else:
        fraud_flag = "safe"
        fraud_score = 0.1
        reason = "Normal transaction amount."

    return FraudDetectionResult(
        transaction_id=str(uuid.uuid4()), # Placeholder
        fraud_score=fraud_score,
        fraud_flag=fraud_flag,
        reason=reason
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
        bill_date: datetime($bill_date),
        due_date: datetime($due_date),
        total_amount: $total_amount,
        status: $status,
        description: $description,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_VENDOR_BILL]->(vb)
    RETURN vb
    """
    params = vendor_bill_data.model_dump()
    params["id"] = bill_neo4j_id
    params["user_id"] = user_id
    params["bill_date"] = vendor_bill_data.bill_date.isoformat()
    params["due_date"] = vendor_bill_data.due_date.isoformat()
    params["total_amount"] = float(vendor_bill_data.total_amount) # Store Decimal as float in Neo4j if needed, or handle conversion

    result = await session.run(query, params)
    record = await result.single()
    vb_node = record["vb"]

    # TODO: Link to Journal Entry automatically?

    return VendorBillInDB(
        id=vb_node["id"],
        user_id=user_id,
        vendor_id=vb_node["vendor_id"],
        bill_number=vb_node["bill_number"],
        bill_date=datetime.fromisoformat(vb_node["bill_date"].iso_format()),
        due_date=datetime.fromisoformat(vb_node["due_date"].iso_format()),
        total_amount=Decimal(str(vb_node["total_amount"])),
        status=vb_node["status"],
        description=vb_node["description"],
        created_at=datetime.fromisoformat(vb_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(vb_node["updated_at"].iso_format()),
    )

# --- Ledger Endpoints ---
async def get_ledger_report(session: AsyncSession, user_id: str, account_number: str, start_date: Optional[datetime] = None, end_date: Optional[datetime] = None) -> LedgerReport:
    account = await get_account(session, user_id, account_number)
    if not account:
        raise NotFoundError(detail=f"Account {account_number} not found for user.", code="ACCOUNT_NOT_FOUND")

    # Initial balance at the start_date (or beginning of time if no start_date)
    initial_balance = Decimal('0.00')
    if start_date:
        initial_balance = await get_account_balance(session, user_id, account_number, as_of_date=start_date - timedelta(microseconds=1))

    # Fetch all journal lines affecting this account within the period
    date_filter = ""
    if start_date:
        date_filter += " AND je.entry_date >= datetime($start_date)"
    if end_date:
        date_filter += " AND je.entry_date <= datetime($end_date)"

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {{account_number: $account_number}})
    WHERE je.status = 'posted' {date_filter}
    RETURN je.id AS entry_id, je.entry_date AS entry_date, je.description AS entry_description,
           jl.debit AS debit, jl.credit AS credit, je.source_module AS source_module
    ORDER BY je.entry_date, je.created_at
    """
    params = {"user_id": user_id, "account_number": account_number}
    if start_date:
        params["start_date"] = start_date.isoformat()
    if end_date:
        params["end_date"] = end_date.isoformat()

    result = await session.run(query, params)
    entries: List[LedgerEntry] = []
    current_balance = initial_balance

    async for record in result:
        debit = Decimal(str(record["debit"]))
        credit = Decimal(str(record["credit"]))

        # Apply normal balance rules to update running balance
        if account.normal_balance == "debit":
            current_balance = current_balance + debit - credit
        else: # normal_balance == "credit"
            current_balance = current_balance - debit + credit
        
        entries.append(LedgerEntry(
            entry_id=record["entry_id"],
            entry_date=datetime.fromisoformat(record["entry_date"].iso_format()),
            description=record["entry_description"],
            debit=debit,
            credit=credit,
            balance=current_balance,
            source_module=record["source_module"]
        ))
    
    end_balance = current_balance # The final running balance after all entries

    return LedgerReport(
        account_number=account_number,
        account_name=account.name,
        normal_balance=account.normal_balance,
        start_balance=initial_balance,
        entries=entries,
        end_balance=end_balance
    )

# --- Trial Balance Endpoints ---
async def get_trial_balance_report(session: AsyncSession, user_id: str, as_of_date: Optional[datetime] = None) -> TrialBalanceReport:
    all_accounts = await get_all_accounts(session, user_id)
    trial_balance_accounts: List[TrialBalanceAccount] = []
    total_debits = Decimal('0.00')
    total_credits = Decimal('0.00')

    for account in all_accounts:
        balance = await get_account_balance(session, user_id, account.account_number, as_of_date)
        
        tb_account = TrialBalanceAccount(
            account_number=account.account_number,
            account_name=account.name,
            account_type=account.account_type,
            debit=Decimal('0.00'),
            credit=Decimal('0.00')
        )

        if account.normal_balance == "debit":
            if balance >= 0:
                tb_account.debit = balance
                total_debits += balance
            else: # If debit-normal account has a credit balance
                tb_account.credit = -balance
                total_credits += -balance
        else: # account.normal_balance == "credit"
            if balance >= 0:
                tb_account.credit = balance
                total_credits += balance
            else: # If credit-normal account has a debit balance
                tb_account.debit = -balance
                total_debits += -balance
        
        trial_balance_accounts.append(tb_account)

    is_balanced = total_debits == total_credits

    return TrialBalanceReport(
        report_date=as_of_date if as_of_date else datetime.utcnow(),
        accounts=trial_balance_accounts,
        total_debits=total_debits,
        total_credits=total_credits,
        is_balanced=is_balanced
    )

# --- Financial Statement Generation ---
async def get_income_statement(session: AsyncSession, user_id: str, start_date: datetime, end_date: datetime) -> IncomeStatement:
    all_accounts = await get_all_accounts(session, user_id)
    
    revenues: List[FinancialStatementLine] = []
    expenses: List[FinancialStatementLine] = []
    
    total_revenues = Decimal('0.00')
    total_expenses = Decimal('0.00')

    for account in all_accounts:
        if account.account_type in ["revenue", "expense"]:
            # Calculate balance for the period (end_date - start_date)
            # Need to get the sum of debits and credits for the period, not just an as_of_date balance
            query = """
            MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {account_number: $account_number})
            WHERE je.status = 'posted' AND je.entry_date >= datetime($start_date) AND je.entry_date <= datetime($end_date)
            RETURN SUM(jl.debit) AS period_debits, SUM(jl.credit) AS period_credits
            """
            params = {"user_id": user_id, "account_number": account.account_number, "start_date": start_date.isoformat(), "end_date": end_date.isoformat()}
            result = await session.run(query, params)
            record = await result.single()

            period_debits = Decimal(str(record["period_debits"])) if record and record["period_debits"] else Decimal('0.00')
            period_credits = Decimal(str(record["period_credits"])) if record and record["period_credits"] else Decimal('0.00')

            period_net_change = Decimal('0.00')
            if account.account_type == "revenue":
                # Revenue increases with credit, decreases with debit
                period_net_change = period_credits - period_debits
                total_revenues += period_net_change
                revenues.append(FinancialStatementLine(category=account.name, amount=period_net_change))
            elif account.account_type == "expense":
                # Expense increases with debit, decreases with credit
                period_net_change = period_debits - period_credits
                total_expenses += period_net_change
                expenses.append(FinancialStatementLine(category=account.name, amount=period_net_change))
    
    net_income = total_revenues - total_expenses

    return IncomeStatement(
        start_date=start_date,
        end_date=end_date,
        revenues=revenues,
        expenses=expenses,
        net_income=net_income
    )

async def get_balance_sheet(session: AsyncSession, user_id: str, as_of_date: datetime) -> BalanceSheet:
    all_accounts = await get_all_accounts(session, user_id)

    assets: List[FinancialStatementLine] = []
    liabilities: List[FinancialStatementLine] = []
    equity: List[FinancialStatementLine] = []

    total_assets = Decimal('0.00')
    total_liabilities = Decimal('0.00')
    total_equity = Decimal('0.00')

    for account in all_accounts:
        if account.account_type in ["asset", "liability", "equity"]:
            balance = await get_account_balance(session, user_id, account.account_number, as_of_date)
            
            if account.account_type == "asset":
                total_assets += balance
                assets.append(FinancialStatementLine(category=account.name, amount=balance))
            elif account.account_type == "liability":
                total_liabilities += balance
                liabilities.append(FinancialStatementLine(category=account.name, amount=balance))
            elif account.account_type == "equity":
                # For equity, consider retained earnings from prior period income statements if available
                # For simplicity in this first pass, we just use the current balance of equity accounts
                total_equity += balance
                equity.append(FinancialStatementLine(category=account.name, amount=balance))
    
    total_liabilities_equity = total_liabilities + total_equity
    is_balanced = total_assets == total_liabilities_equity

    return BalanceSheet(
        as_of_date=as_of_date,
        assets=assets,
        liabilities=liabilities,
        equity=equity,
        total_assets=total_assets,
        total_liabilities=total_liabilities,
        total_equity=total_equity,
        total_liabilities_equity=total_liabilities_equity # Should equal total_assets
    )


async def get_all_journal_entries_for_account_type(session: AsyncSession, user_id: str, account_type: str, start_date: datetime, end_date: datetime) -> List[JournalEntryInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_JOURNAL_ENTRY]->(je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:IMPACTS]->(a:Account {account_type: $account_type})
    WHERE je.status = 'posted' AND je.entry_date >= datetime($start_date) AND je.entry_date <= datetime($end_date)
    RETURN je, COLLECT({
        id: jl.id,
        account_number: a.account_number,
        debit: jl.debit,
        credit: jl.credit,
        description: jl.description
    }) AS lines_data
    ORDER BY je.entry_date, je.created_at
    """
    params = {
        "user_id": user_id,
        "account_type": account_type,
        "start_date": start_date.isoformat(),
        "end_date": end_date.isoformat()
    }
    result = await session.run(query, params)
    
    journal_entries = []
    async for record in result:
        je_node = record["je"]
        lines_data = record["lines_data"]
        lines = [JournalLineBase(
            account_number=line["account_number"],
            debit=Decimal(str(line["debit"])),
            credit=Decimal(str(line["credit"])),
            description=line["description"]
        ) for line in lines_data if line["account_number"] is not None]

        journal_entries.append(JournalEntryInDB(
            id=je_node["id"],
            user_id=user_id,
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            status=je_node["status"],
            fraud_flag=je_node.get("fraud_flag", "safe"),
            fraud_score=je_node.get("fraud_score"),
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            lines=lines
        ))
    return journal_entries