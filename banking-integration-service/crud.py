from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from banking_integration_service.models import (
    BankConnectionCreate, BankConnectionUpdate, BankConnectionInDB,
    BankAccountCreate, BankAccountUpdate, BankAccountInDB,
    BankTransactionCreate, BankTransactionUpdate, BankTransactionInDB,
    TransactionCategorizationRuleCreate, TransactionCategorizationRuleUpdate, TransactionCategorizationRuleInDB,
    ReconciliationMatchCreate, ReconciliationMatchUpdate, ReconciliationMatchInDB # NEW
)
from datetime import datetime, date, timezone
import uuid
from decimal import Decimal
from pydantic import BaseModel # Import BaseModel for _to_neo4j_props helper

# Helper function to convert Pydantic models to Neo4j-compatible dictionary
def _to_neo4j_props(model_instance: BaseModel) -> Dict[str, Any]:
    data = model_instance.model_dump()
    for key, value in data.items():
        if isinstance(value, datetime) or isinstance(value, date):
            data[key] = value.isoformat()
        elif isinstance(value, Decimal):
            data[key] = str(value) # Store Decimal as string
    return data

# Helper function to reconstruct Pydantic models from Neo4j properties
def _from_neo4j_props(node_props: Dict[str, Any], model_class: BaseModel) -> BaseModel:
    props = node_props.copy()
    for key, value in props.items():
        if (key.endswith('_at') or key.endswith('_date')) and isinstance(value, str):
            try:
                if 'T' in value: # datetime
                    props[key] = datetime.fromisoformat(value)
                else: # date
                    props[key] = date.fromisoformat(value)
            except ValueError:
                pass # Keep as string if parsing fails
        elif key in ['amount', 'current_balance', 'available_balance', 'matched_amount'] and isinstance(value, str):
            try:
                props[key] = Decimal(value)
            except:
                pass # Keep as string if parsing fails
    return model_class(**props)


# --- BankConnection CRUD ---
async def create_bank_connection(session: AsyncSession, connection_data: BankConnectionCreate) -> BankConnectionInDB:
    connection_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(connection_data)
    props["id"] = connection_id
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (bc:BankConnection $props)
    CREATE (u)-[:OWNS_CONNECTION]->(bc)
    RETURN bc
    """
    result = await session.run(query, user_id=connection_data.user_id, props=props)
    record = await result.single()
    return _from_neo4j_props(record["bc"], BankConnectionInDB)

async def get_bank_connection(session: AsyncSession, connection_id: str) -> Optional[BankConnectionInDB]:
    query = """
    MATCH (bc:BankConnection {id: $connection_id})
    RETURN bc
    """
    result = await session.run(query, connection_id=connection_id)
    record = await result.single()
    return _from_neo4j_props(record["bc"], BankConnectionInDB) if record else None

async def get_user_bank_connections(session: AsyncSession, user_id: str) -> List[BankConnectionInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CONNECTION]->(bc:BankConnection)
    RETURN bc
    ORDER BY bc.updated_at DESC
    """
    result = await session.run(query, user_id=user_id)
    return [_from_neo4j_props(record["bc"], BankConnectionInDB) async for record in result]

async def update_bank_connection(session: AsyncSession, connection_id: str, connection_data: BankConnectionUpdate) -> Optional[BankConnectionInDB]:
    update_fields = connection_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_bank_connection(session, connection_id)
    
    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    # Fetch existing data to merge for _to_neo4j_props conversion if needed (Pydantic updates)
    existing_conn = await get_bank_connection(session, connection_id)
    if not existing_conn:
        return None
    merged_data = existing_conn.model_dump()
    merged_data.update(update_fields)
    
    props_for_update = _to_neo4j_props(models.BankConnectionBase(**merged_data))
    
    set_clauses = [f"bc.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)
    query = f"""
    MATCH (bc:BankConnection {{id: $connection_id}})
    SET {set_query_part}
    RETURN bc
    """
    result = await session.run(query, connection_id=connection_id, **update_fields)
    record = await result.single()
    return _from_neo4j_props(record["bc"], BankConnectionInDB) if record else None

async def delete_bank_connection(session: AsyncSession, connection_id: str) -> bool:
    query = """
    MATCH (bc:BankConnection {id: $connection_id})
    DETACH DELETE bc
    """
    result = await session.run(query, connection_id=connection_id)
    return result.consume().counters.nodes_deleted > 0


# --- BankAccount CRUD ---
async def create_bank_account(session: AsyncSession, account_data: BankAccountCreate) -> BankAccountInDB:
    account_uuid = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(account_data)
    props["id"] = account_uuid
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    MATCH (bc:BankConnection {id: $connection_id})
    CREATE (ba:BankAccount $props)
    CREATE (bc)-[:HAS_ACCOUNT]->(ba)
    RETURN ba
    """
    result = await session.run(query, connection_id=account_data.connection_id, props=props)
    record = await result.single()
    return _from_neo4j_props(record["ba"], BankAccountInDB)

async def get_bank_account(session: AsyncSession, account_id: str) -> Optional[BankAccountInDB]:
    query = """
    MATCH (ba:BankAccount {id: $account_id})
    RETURN ba
    """
    result = await session.run(query, account_id=account_id)
    record = await result.single()
    return _from_neo4j_props(record["ba"], BankAccountInDB) if record else None

async def get_connection_bank_accounts(session: AsyncSession, connection_id: str) -> List[BankAccountInDB]:
    query = """
    MATCH (bc:BankConnection {id: $connection_id})-[:HAS_ACCOUNT]->(ba:BankAccount)
    RETURN ba
    ORDER BY ba.name
    """
    result = await session.run(query, connection_id=connection_id)
    return [_from_neo4j_props(record["ba"], BankAccountInDB) async for record in result]

async def update_bank_account(session: AsyncSession, account_id: str, account_data: BankAccountUpdate) -> Optional[BankAccountInDB]:
    update_fields = account_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_bank_account(session, account_id)
    
    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    existing_account = await get_bank_account(session, account_id)
    if not existing_account:
        return None
    merged_data = existing_account.model_dump()
    merged_data.update(update_fields)
    
    props_for_update = _to_neo4j_props(models.BankAccountBase(**merged_data))

    set_clauses = [f"ba.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)
    query = f"""
    MATCH (ba:BankAccount {{id: $account_id}})
    SET {set_query_part}
    RETURN ba
    """
    result = await session.run(query, account_id=account_id, **update_fields)
    record = await result.single()
    return _from_neo4j_props(record["ba"], BankAccountInDB) if record else None

async def delete_bank_account(session: AsyncSession, account_id: str) -> bool:
    query = """
    MATCH (ba:BankAccount {id: $account_id})
    DETACH DELETE ba
    """
    result = await session.run(query, account_id=account_id)
    return result.consume().counters.nodes_deleted > 0


# --- BankTransaction CRUD ---
async def create_bank_transaction(session: AsyncSession, transaction_data: BankTransactionCreate) -> BankTransactionInDB:
    transaction_uuid = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(transaction_data)
    props["id"] = transaction_uuid
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    MATCH (ba:BankAccount {id: $account_id})
    CREATE (bt:BankTransaction $props)
    CREATE (ba)-[:HAS_TRANSACTION]->(bt)
    RETURN bt
    """
    result = await session.run(query, account_id=transaction_data.account_id, props=props)
    record = await result.single()
    return _from_neo4j_props(record["bt"], BankTransactionInDB)

async def get_bank_transaction(session: AsyncSession, transaction_id: str) -> Optional[BankTransactionInDB]:
    query = """
    MATCH (bt:BankTransaction {id: $transaction_id})
    RETURN bt
    """
    result = await session.run(query, transaction_id=transaction_id)
    record = await result.single()
    return _from_neo4j_props(record["bt"], BankTransactionInDB) if record else None

async def get_account_bank_transactions(session: AsyncSession, account_id: str) -> List[BankTransactionInDB]:
    query = """
    MATCH (ba:BankAccount {id: $account_id})-[:HAS_TRANSACTION]->(bt:BankTransaction)
    RETURN bt
    ORDER BY bt.date DESC
    """
    result = await session.run(query, account_id=account_id)
    return [_from_neo4j_props(record["bt"], BankTransactionInDB) async for record in result]

async def update_bank_transaction(session: AsyncSession, transaction_id: str, transaction_data: BankTransactionUpdate) -> Optional[BankTransactionInDB]:
    update_fields = transaction_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_bank_transaction(session, transaction_id)
    
    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    existing_transaction = await get_bank_transaction(session, transaction_id)
    if not existing_transaction:
        return None
    merged_data = existing_transaction.model_dump()
    merged_data.update(update_fields)

    props_for_update = _to_neo4j_props(models.BankTransactionBase(**merged_data))

    set_clauses = [f"bt.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)
    query = f"""
    MATCH (bt:BankTransaction {{id: $transaction_id}})
    SET {set_query_part}
    RETURN bt
    """
    result = await session.run(query, transaction_id=transaction_id, **update_fields)
    record = await result.single()
    return _from_neo4j_props(record["bt"], BankTransactionInDB) if record else None

async def delete_bank_transaction(session: AsyncSession, transaction_id: str) -> bool:
    query = """
    MATCH (bt:BankTransaction {id: $transaction_id})
    DETACH DELETE bt
    """
    result = await session.run(query, transaction_id=transaction_id)
    return result.consume().counters.nodes_deleted > 0


# --- TransactionCategorizationRule CRUD ---
async def create_categorization_rule(session: AsyncSession, rule_data: TransactionCategorizationRuleCreate) -> TransactionCategorizationRuleInDB:
    rule_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(rule_data)
    props["id"] = rule_id
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (r:TransactionCategorizationRule $props)
    CREATE (u)-[:DEFINES_RULE]->(r)
    RETURN r
    """
    result = await session.run(query, user_id=rule_data.user_id, props=props)
    record = await result.single()
    return _from_neo4j_props(record["r"], TransactionCategorizationRuleInDB)

async def get_categorization_rule(session: AsyncSession, rule_id: str) -> Optional[TransactionCategorizationRuleInDB]:
    query = """
    MATCH (r:TransactionCategorizationRule {id: $rule_id})
    RETURN r
    """
    result = await session.run(query, rule_id=rule_id)
    record = await result.single()
    return _from_neo4j_props(record["r"], TransactionCategorizationRuleInDB) if record else None

async def get_user_categorization_rules(session: AsyncSession, user_id: str) -> List[TransactionCategorizationRuleInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:DEFINES_RULE]->(r:TransactionCategorizationRule)
    RETURN r
    ORDER BY r.priority, r.rule_name
    """
    result = await session.run(query, user_id=user_id)
    return [_from_neo4j_props(record["r"], TransactionCategorizationRuleInDB) async for record in result]

async def update_categorization_rule(session: AsyncSession, rule_id: str, rule_data: TransactionCategorizationRuleUpdate) -> Optional[TransactionCategorizationRuleInDB]:
    update_fields = rule_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_categorization_rule(session, rule_id)
    
    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    existing_rule = await get_categorization_rule(session, rule_id)
    if not existing_rule:
        return None
    merged_data = existing_rule.model_dump()
    merged_data.update(update_fields)

    props_for_update = _to_neo4j_props(models.TransactionCategorizationRuleBase(**merged_data))

    set_clauses = [f"r.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)
    query = f"""
    MATCH (r:TransactionCategorizationRule {{id: $rule_id}})
    SET {set_query_part}
    RETURN r
    """
    result = await session.run(query, rule_id=rule_id, **update_fields)
    record = await result.single()
    return _from_neo4j_props(record["r"], TransactionCategorizationRuleInDB) if record else None

async def delete_categorization_rule(session: AsyncSession, rule_id: str) -> bool:
    query = """
    MATCH (r:TransactionCategorizationRule {id: $rule_id})
    DETACH DELETE r
    """
    result = await session.run(query, rule_id=rule_id)
    return result.consume().counters.nodes_deleted > 0


# --- ReconciliationMatch CRUD (NEW) ---
async def create_reconciliation_match(session: AsyncSession, match_data: ReconciliationMatchCreate) -> ReconciliationMatchInDB:
    match_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = _to_neo4j_props(match_data)
    props["id"] = match_id
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    MATCH (bt:BankTransaction {id: $bank_transaction_id})
    MATCH (je:JournalEntry {id: $finacc_journal_entry_id})
    CREATE (rm:ReconciliationMatch $props)
    CREATE (bt)-[:MATCHED_TO]->(rm)
    CREATE (rm)-[:MATCHES_ENTRY]->(je)
    RETURN rm
    """
    result = await session.run(query, bank_transaction_id=match_data.bank_transaction_id, finacc_journal_entry_id=match_data.finacc_journal_entry_id, props=props)
    record = await result.single()
    return _from_neo4j_props(record["rm"], ReconciliationMatchInDB)

async def get_reconciliation_match(session: AsyncSession, match_id: str) -> Optional[ReconciliationMatchInDB]:
    query = """
    MATCH (rm:ReconciliationMatch {id: $match_id})
    RETURN rm
    """
    result = await session.run(query, match_id=match_id)
    record = await result.single()
    return _from_neo4j_props(record["rm"], ReconciliationMatchInDB) if record else None

async def get_matches_for_bank_transaction(session: AsyncSession, bank_transaction_id: str) -> List[ReconciliationMatchInDB]:
    query = """
    MATCH (bt:BankTransaction {id: $bank_transaction_id})-[:MATCHED_TO]->(rm:ReconciliationMatch)
    RETURN rm
    ORDER BY rm.created_at DESC
    """
    result = await session.run(query, bank_transaction_id=bank_transaction_id)
    return [_from_neo4j_props(record["rm"], ReconciliationMatchInDB) async for record in result]

async def get_matches_for_journal_entry(session: AsyncSession, finacc_journal_entry_id: str) -> List[ReconciliationMatchInDB]:
    query = """
    MATCH (rm:ReconciliationMatch)-[:MATCHES_ENTRY]->(je:JournalEntry {id: $finacc_journal_entry_id})
    RETURN rm
    ORDER BY rm.created_at DESC
    """
    result = await session.run(query, finacc_journal_entry_id=finacc_journal_entry_id)
    return [_from_neo4j_props(record["rm"], ReconciliationMatchInDB) async for record in result]

async def update_reconciliation_match(session: AsyncSession, match_id: str, match_data: ReconciliationMatchUpdate) -> Optional[ReconciliationMatchInDB]:
    update_fields = match_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_reconciliation_match(session, match_id)
    
    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    existing_match = await get_reconciliation_match(session, match_id)
    if not existing_match:
        return None
    merged_data = existing_match.model_dump()
    merged_data.update(update_fields)

    props_for_update = _to_neo4j_props(models.ReconciliationMatchBase(**merged_data))

    set_clauses = [f"rm.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)
    query = f"""
    MATCH (rm:ReconciliationMatch {{id: $match_id}})
    SET {set_query_part}
    RETURN rm
    """
    result = await session.run(query, match_id=match_id, **update_fields)
    record = await result.single()
    return _from_neo4j_props(record["rm"], ReconciliationMatchInDB) if record else None

async def delete_reconciliation_match(session: AsyncSession, match_id: str) -> bool:
    query = """
    MATCH (rm:ReconciliationMatch {id: $match_id})
    DETACH DELETE rm
    """
    result = await session.run(query, match_id=match_id)
    return result.consume().counters.nodes_deleted > 0
