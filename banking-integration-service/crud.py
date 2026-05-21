from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from banking_integration_service.models import (
    BankConnectionCreate, BankConnectionUpdate, BankConnectionInDB,
    BankAccountCreate, BankAccountUpdate, BankAccountInDB,
    BankTransactionCreate, BankTransactionUpdate, BankTransactionInDB,
    TransactionCategorizationRuleCreate, TransactionCategorizationRuleUpdate, TransactionCategorizationRuleInDB,
    ReconciliationMatchCreate, ReconciliationMatchUpdate, ReconciliationMatchInDB
)
from datetime import datetime
import uuid

# --- BankConnection CRUD ---
async def create_bank_connection(session: AsyncSession, connection_data: BankConnectionCreate) -> BankConnectionInDB:
    connection_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (bc:BankConnection {
        id: $id,
        provider: $provider,
        access_token: $access_token,
        external_id: $external_id,
        status: $status,
        last_synced_at: datetime($last_synced_at) ON CREATE NULL,
        metadata: $metadata,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_BANK_CONNECTION]->(bc)
    RETURN bc
    """
    params = connection_data.model_dump()
    params["id"] = connection_id
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()
    if params["last_synced_at"]:
        params["last_synced_at"] = params["last_synced_at"].isoformat()

    result = await session.run(query, params)
    record = await result.single()
    bc_node = record["bc"]

    return BankConnectionInDB(
        id=bc_node["id"],
        user_id=connection_data.user_id,
        provider=bc_node["provider"],
        access_token=bc_node["access_token"],
        external_id=bc_node["external_id"],
        status=bc_node["status"],
        last_synced_at=datetime.fromisoformat(bc_node["last_synced_at"].iso_format()) if bc_node.get("last_synced_at") else None,
        metadata=bc_node["metadata"],
        created_at=datetime.fromisoformat(bc_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(bc_node["updated_at"].iso_format()),
    )

async def get_bank_connection(session: AsyncSession, user_id: str, connection_id: str) -> Optional[BankConnectionInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BANK_CONNECTION]->(bc:BankConnection {id: $connection_id})
    RETURN bc
    """
    result = await session.run(query, user_id=user_id, connection_id=connection_id)
    record = await result.single()

    if record:
        bc_node = record["bc"]
        return BankConnectionInDB(
            id=bc_node["id"],
            user_id=user_id,
            provider=bc_node["provider"],
            access_token=bc_node["access_token"],
            external_id=bc_node["external_id"],
            status=bc_node["status"],
            last_synced_at=datetime.fromisoformat(bc_node["last_synced_at"].iso_format()) if bc_node.get("last_synced_at") else None,
            metadata=bc_node["metadata"],
            created_at=datetime.fromisoformat(bc_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(bc_node["updated_at"].iso_format()),
        )
    return None

async def get_all_bank_connections(session: AsyncSession, user_id: str) -> List[BankConnectionInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BANK_CONNECTION]->(bc:BankConnection)
    RETURN bc
    """
    result = await session.run(query, user_id=user_id)
    connections = []
    async for record in result:
        bc_node = record["bc"]
        connections.append(BankConnectionInDB(
            id=bc_node["id"],
            user_id=user_id,
            provider=bc_node["provider"],
            access_token=bc_node["access_token"],
            external_id=bc_node["external_id"],
            status=bc_node["status"],
            last_synced_at=datetime.fromisoformat(bc_node["last_synced_at"].iso_format()) if bc_node.get("last_synced_at") else None,
            metadata=bc_node["metadata"],
            created_at=datetime.fromisoformat(bc_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(bc_node["updated_at"].iso_format()),
        ))
    return connections

async def update_bank_connection(session: AsyncSession, user_id: str, connection_id: str, connection_data: BankConnectionUpdate) -> Optional[BankConnectionInDB]:
    update_fields = connection_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_bank_connection(session, user_id, connection_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "last_synced_at" in update_fields and update_fields["last_synced_at"]:
        update_fields["last_synced_at"] = update_fields["last_synced_at"].isoformat()

    set_clauses = [f"bc.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BANK_CONNECTION]->(bc:BankConnection {{id: $connection_id}})
    SET {set_query_part}
    RETURN bc
    """
    params = {"user_id": user_id, "connection_id": connection_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_bank_connection(session, user_id, connection_id)
    return None

async def delete_bank_connection(session: AsyncSession, user_id: str, connection_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BANK_CONNECTION]->(bc:BankConnection {id: $connection_id})
    DETACH DELETE bc
    """
    result = await session.run(query, user_id=user_id, connection_id=connection_id)
    return result.consume().counters.nodes_deleted > 0

# --- BankAccount CRUD ---
async def create_bank_account(session: AsyncSession, connection_id: str, account_data: BankAccountCreate) -> BankAccountInDB:
    account_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (bc:BankConnection {id: $connection_id})
    CREATE (ba:BankAccount {
        id: $id,
        account_id: $account_id,
        connection_id: $connection_id,
        name: $name,
        mask: $mask,
        type: $type,
        subtype: $subtype,
        currency: $currency,
        current_balance: $current_balance,
        available_balance: $available_balance,
        finacc_account_number: $finacc_account_number,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (bc)-[:HAS_ACCOUNT]->(ba)
    RETURN ba
    """
    params = account_data.model_dump()
    params["id"] = account_neo4j_id
    params["current_balance"] = float(params["current_balance"])
    if params["available_balance"]:
        params["available_balance"] = float(params["available_balance"])
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    ba_node = record["ba"]

    return BankAccountInDB(
        id=ba_node["id"],
        account_id=ba_node["account_id"],
        connection_id=ba_node["connection_id"],
        name=ba_node["name"],
        mask=ba_node["mask"],
        type=ba_node["type"],
        subtype=ba_node["subtype"],
        currency=ba_node["currency"],
        current_balance=ba_node["current_balance"],
        available_balance=ba_node["available_balance"],
        finacc_account_number=ba_node["finacc_account_number"],
        status=ba_node["status"],
        created_at=datetime.fromisoformat(ba_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(ba_node["updated_at"].iso_format()),
    )

async def get_bank_account(session: AsyncSession, account_id: str) -> Optional[BankAccountInDB]:
    query = """
    MATCH (ba:BankAccount {id: $account_id})
    RETURN ba
    """
    result = await session.run(query, account_id=account_id)
    record = await result.single()

    if record:
        ba_node = record["ba"]
        return BankAccountInDB(
            id=ba_node["id"],
            account_id=ba_node["account_id"],
            connection_id=ba_node["connection_id"],
            name=ba_node["name"],
            mask=ba_node["mask"],
            type=ba_node["type"],
            subtype=ba_node["subtype"],
            currency=ba_node["currency"],
            current_balance=ba_node["current_balance"],
            available_balance=ba_node["available_balance"],
            finacc_account_number=ba_node["finacc_account_number"],
            status=ba_node["status"],
            created_at=datetime.fromisoformat(ba_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(ba_node["updated_at"].iso_format()),
        )
    return None

async def get_bank_accounts_for_connection(session: AsyncSession, connection_id: str) -> List[BankAccountInDB]:
    query = """
    MATCH (bc:BankConnection {id: $connection_id})-[:HAS_ACCOUNT]->(ba:BankAccount)
    RETURN ba
    """
    result = await session.run(query, connection_id=connection_id)
    accounts = []
    async for record in result:
        ba_node = record["ba"]
        accounts.append(BankAccountInDB(
            id=ba_node["id"],
            account_id=ba_node["account_id"],
            connection_id=ba_node["connection_id"],
            name=ba_node["name"],
            mask=ba_node["mask"],
            type=ba_node["type"],
            subtype=ba_node["subtype"],
            currency=ba_node["currency"],
            current_balance=ba_node["current_balance"],
            available_balance=ba_node["available_balance"],
            finacc_account_number=ba_node["finacc_account_number"],
            status=ba_node["status"],
            created_at=datetime.fromisoformat(ba_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(ba_node["updated_at"].iso_format()),
        ))
    return accounts

async def update_bank_account(session: AsyncSession, account_id: str, account_data: BankAccountUpdate) -> Optional[BankAccountInDB]:
    update_fields = account_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_bank_account(session, account_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "current_balance" in update_fields:
        update_fields["current_balance"] = float(update_fields["current_balance"])
    if "available_balance" in update_fields and update_fields["available_balance"]:
        update_fields["available_balance"] = float(update_fields["available_balance"])

    set_clauses = [f"ba.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (ba:BankAccount {{id: $account_id}})
    SET {set_query_part}
    RETURN ba
    """
    params = {"account_id": account_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_bank_account(session, account_id)
    return None

async def delete_bank_account(session: AsyncSession, account_id: str) -> bool:
    query = """
    MATCH (ba:BankAccount {id: $account_id})
    DETACH DELETE ba
    """
    result = await session.run(query, account_id=account_id)
    return result.consume().counters.nodes_deleted > 0

# --- BankTransaction CRUD ---
async def create_bank_transaction(session: AsyncSession, account_id: str, transaction_data: BankTransactionCreate) -> BankTransactionInDB:
    transaction_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (ba:BankAccount {id: $account_id})
    CREATE (bt:BankTransaction {
        id: $id,
        transaction_id: $transaction_id,
        account_id: $account_id,
        description: $description,
        amount: $amount,
        date: date($date),
        posted_date: date($posted_date) ON CREATE NULL,
        category: $category,
        type: $type,
        status: $status,
        finacc_journal_entry_id: $finacc_journal_entry_id,
        is_reconciled: $is_reconciled,
        metadata: $metadata,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (ba)-[:HAS_TRANSACTION]->(bt)
    RETURN bt
    """
    params = transaction_data.model_dump()
    params["id"] = transaction_neo4j_id
    params["amount"] = float(params["amount"])
    params["date"] = params["date"].isoformat()
    if params["posted_date"]:
        params["posted_date"] = params["posted_date"].isoformat()
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    bt_node = record["bt"]

    return BankTransactionInDB(
        id=bt_node["id"],
        transaction_id=bt_node["transaction_id"],
        account_id=bt_node["account_id"],
        description=bt_node["description"],
        amount=bt_node["amount"],
        date=bt_node["date"],
        posted_date=bt_node["posted_date"],
        category=bt_node["category"],
        type=bt_node["type"],
        status=bt_node["status"],
        finacc_journal_entry_id=bt_node["finacc_journal_entry_id"],
        is_reconciled=bt_node["is_reconciled"],
        metadata=bt_node["metadata"],
        created_at=datetime.fromisoformat(bt_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(bt_node["updated_at"].iso_format()),
    )

async def get_bank_transaction(session: AsyncSession, transaction_id: str) -> Optional[BankTransactionInDB]:
    query = """
    MATCH (bt:BankTransaction {id: $transaction_id})
    RETURN bt
    """
    result = await session.run(query, transaction_id=transaction_id)
    record = await result.single()

    if record:
        bt_node = record["bt"]
        return BankTransactionInDB(
            id=bt_node["id"],
            transaction_id=bt_node["transaction_id"],
            account_id=bt_node["account_id"],
            description=bt_node["description"],
            amount=bt_node["amount"],
            date=bt_node["date"],
            posted_date=bt_node["posted_date"],
            category=bt_node["category"],
            type=bt_node["type"],
            status=bt_node["status"],
            finacc_journal_entry_id=bt_node["finacc_journal_entry_id"],
            is_reconciled=bt_node["is_reconciled"],
            metadata=bt_node["metadata"],
            created_at=datetime.fromisoformat(bt_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(bt_node["updated_at"].iso_format()),
        )
    return None

async def get_bank_transactions_for_account(session: AsyncSession, account_id: str) -> List[BankTransactionInDB]:
    query = """
    MATCH (ba:BankAccount {id: $account_id})-[:HAS_TRANSACTION]->(bt:BankTransaction)
    RETURN bt
    ORDER BY bt.date DESC
    """
    result = await session.run(query, account_id=account_id)
    transactions = []
    async for record in result:
        bt_node = record["bt"]
        transactions.append(BankTransactionInDB(
            id=bt_node["id"],
            transaction_id=bt_node["transaction_id"],
            account_id=bt_node["account_id"],
            description=bt_node["description"],
            amount=bt_node["amount"],
            date=bt_node["date"],
            posted_date=bt_node["posted_date"],
            category=bt_node["category"],
            type=bt_node["type"],
            status=bt_node["status"],
            finacc_journal_entry_id=bt_node["finacc_journal_entry_id"],
            is_reconciled=bt_node["is_reconciled"],
            metadata=bt_node["metadata"],
            created_at=datetime.fromisoformat(bt_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(bt_node["updated_at"].iso_format()),
        ))
    return transactions

async def update_bank_transaction(session: AsyncSession, transaction_id: str, transaction_data: BankTransactionUpdate) -> Optional[BankTransactionInDB]:
    update_fields = transaction_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_bank_transaction(session, transaction_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()

    set_clauses = [f"bt.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (bt:BankTransaction {{id: $transaction_id}})
    SET {set_query_part}
    RETURN bt
    """
    params = {"transaction_id": transaction_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_bank_transaction(session, transaction_id)
    return None

# --- TransactionCategorizationRule CRUD ---
async def create_categorization_rule(session: AsyncSession, user_id: str, rule_data: TransactionCategorizationRuleCreate) -> TransactionCategorizationRuleInDB:
    rule_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (tcr:TransactionCategorizationRule {
        id: $id,
        user_id: $user_id,
        rule_name: $rule_name,
        match_field: $match_field,
        match_pattern: $match_pattern,
        target_category: $target_category,
        target_finacc_account_number: $target_finacc_account_number,
        is_active: $is_active,
        priority: $priority,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:DEFINES_RULE]->(tcr)
    RETURN tcr
    """
    params = rule_data.model_dump()
    params["id"] = rule_id
    params["user_id"] = user_id
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    tcr_node = record["tcr"]

    return TransactionCategorizationRuleInDB(
        id=tcr_node["id"],
        user_id=tcr_node["user_id"],
        rule_name=tcr_node["rule_name"],
        match_field=tcr_node["match_field"],
        match_pattern=tcr_node["match_pattern"],
        target_category=tcr_node["target_category"],
        target_finacc_account_number=tcr_node["target_finacc_account_number"],
        is_active=tcr_node["is_active"],
        priority=tcr_node["priority"],
        created_at=datetime.fromisoformat(tcr_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(tcr_node["updated_at"].iso_format()),
    )

async def get_categorization_rule(session: AsyncSession, user_id: str, rule_id: str) -> Optional[TransactionCategorizationRuleInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:DEFINES_RULE]->(tcr:TransactionCategorizationRule {id: $rule_id})
    RETURN tcr
    """
    result = await session.run(query, user_id=user_id, rule_id=rule_id)
    record = await result.single()

    if record:
        tcr_node = record["tcr"]
        return TransactionCategorizationRuleInDB(
            id=tcr_node["id"],
            user_id=tcr_node["user_id"],
            rule_name=tcr_node["rule_name"],
            match_field=tcr_node["match_field"],
            match_pattern=tcr_node["match_pattern"],
            target_category=tcr_node["target_category"],
            target_finacc_account_number=tcr_node["target_finacc_account_number"],
            is_active=tcr_node["is_active"],
            priority=tcr_node["priority"],
            created_at=datetime.fromisoformat(tcr_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(tcr_node["updated_at"].iso_format()),
        )
    return None

async def get_all_categorization_rules(session: AsyncSession, user_id: str) -> List[TransactionCategorizationRuleInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:DEFINES_RULE]->(tcr:TransactionCategorizationRule)
    RETURN tcr
    ORDER BY tcr.priority ASC, tcr.rule_name ASC
    """
    result = await session.run(query, user_id=user_id)
    rules = []
    async for record in result:
        tcr_node = record["tcr"]
        rules.append(TransactionCategorizationRuleInDB(
            id=tcr_node["id"],
            user_id=tcr_node["user_id"],
            rule_name=tcr_node["rule_name"],
            match_field=tcr_node["match_field"],
            match_pattern=tcr_node["match_pattern"],
            target_category=tcr_node["target_category"],
            target_finacc_account_number=tcr_node["target_finacc_account_number"],
            is_active=tcr_node["is_active"],
            priority=tcr_node["priority"],
            created_at=datetime.fromisoformat(tcr_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(tcr_node["updated_at"].iso_format()),
        ))
    return rules

async def update_categorization_rule(session: AsyncSession, user_id: str, rule_id: str, rule_data: TransactionCategorizationRuleUpdate) -> Optional[TransactionCategorizationRuleInDB]:
    update_fields = rule_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_categorization_rule(session, user_id, rule_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()

    set_clauses = [f"tcr.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:DEFINES_RULE]->(tcr:TransactionCategorizationRule {{id: $rule_id}})
    SET {set_query_part}
    RETURN tcr
    """
    params = {"user_id": user_id, "rule_id": rule_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_categorization_rule(session, user_id, rule_id)
    return None

async def delete_categorization_rule(session: AsyncSession, user_id: str, rule_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:DEFINES_RULE]->(tcr:TransactionCategorizationRule {id: $rule_id})
    DETACH DELETE tcr
    """
    result = await session.run(query, user_id=user_id, rule_id=rule_id)
    return result.consume().counters.nodes_deleted > 0

# --- ReconciliationMatch CRUD ---
async def create_reconciliation_match(session: AsyncSession, match_data: ReconciliationMatchCreate) -> ReconciliationMatchInDB:
    match_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (bt:BankTransaction {id: $bank_transaction_id})
    MATCH (je:JournalEntry {id: $finacc_journal_entry_id})
    CREATE (rm:ReconciliationMatch {
        id: $id,
        bank_transaction_id: $bank_transaction_id,
        finacc_journal_entry_id: $finacc_journal_entry_id,
        match_type: $match_type,
        matched_amount: $matched_amount,
        matched_date: date($matched_date),
        is_confirmed: $is_confirmed,
        confirmed_by_user_id: $confirmed_by_user_id,
        confirmed_at: datetime($confirmed_at) ON CREATE NULL,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (rm)-[:MATCHES_BANK_TRANSACTION]->(bt)
    CREATE (rm)-[:MATCHES_FINACC_JOURNAL_ENTRY]->(je)
    RETURN rm
    """
    params = match_data.model_dump()
    params["id"] = match_id
    params["matched_amount"] = float(params["matched_amount"])
    params["matched_date"] = params["matched_date"].isoformat()
    if params["confirmed_at"]:
        params["confirmed_at"] = params["confirmed_at"].isoformat()
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    rm_node = record["rm"]

    return ReconciliationMatchInDB(
        id=rm_node["id"],
        bank_transaction_id=rm_node["bank_transaction_id"],
        finacc_journal_entry_id=rm_node["finacc_journal_entry_id"],
        match_type=rm_node["match_type"],
        matched_amount=rm_node["matched_amount"],
        matched_date=rm_node["matched_date"],
        is_confirmed=rm_node["is_confirmed"],
        confirmed_by_user_id=rm_node["confirmed_by_user_id"],
        confirmed_at=datetime.fromisoformat(rm_node["confirmed_at"].iso_format()) if rm_node.get("confirmed_at") else None,
        created_at=datetime.fromisoformat(rm_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(rm_node["updated_at"].iso_format()),
    )

async def get_reconciliation_match(session: AsyncSession, match_id: str) -> Optional[ReconciliationMatchInDB]:
    query = """
    MATCH (rm:ReconciliationMatch {id: $match_id})
    RETURN rm
    """
    result = await session.run(query, match_id=match_id)
    record = await result.single()

    if record:
        rm_node = record["rm"]
        return ReconciliationMatchInDB(
            id=rm_node["id"],
            bank_transaction_id=rm_node["bank_transaction_id"],
            finacc_journal_entry_id=rm_node["finacc_journal_entry_id"],
            match_type=rm_node["match_type"],
            matched_amount=rm_node["matched_amount"],
            matched_date=rm_node["matched_date"],
            is_confirmed=rm_node["is_confirmed"],
            confirmed_by_user_id=rm_node["confirmed_by_user_id"],
            confirmed_at=datetime.fromisoformat(rm_node["confirmed_at"].iso_format()) if rm_node.get("confirmed_at") else None,
            created_at=datetime.fromisoformat(rm_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(rm_node["updated_at"].iso_format()),
        )
    return None

async def update_reconciliation_match(session: AsyncSession, match_id: str, match_data: ReconciliationMatchUpdate) -> Optional[ReconciliationMatchInDB]:
    update_fields = match_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_reconciliation_match(session, match_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "confirmed_at" in update_fields and update_fields["confirmed_at"]:
        update_fields["confirmed_at"] = update_fields["confirmed_at"].isoformat()

    set_clauses = [f"rm.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (rm:ReconciliationMatch {{id: $match_id}})
    SET {set_query_part}
    RETURN rm
    """
    params = {"match_id": match_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_reconciliation_match(session, match_id)
    return None

async def delete_reconciliation_match(session: AsyncSession, match_id: str) -> bool:
    query = """
    MATCH (rm:ReconciliationMatch {id: $match_id})
    DETACH DELETE rm
    """
    result = await session.run(query, match_id=match_id)
    return result.consume().counters.nodes_deleted > 0
