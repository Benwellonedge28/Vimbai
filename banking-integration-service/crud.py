from neo4j import AsyncSession
from typing import Optional, List
from banking_integration_service.models import (
    BankAccountCreate, BankAccountUpdate, BankAccountInDB,
    BankTransactionCreate, BankTransactionInDB
)
from datetime import datetime, timedelta
import uuid
from decimal import Decimal

# --- BankAccount CRUD ---
async def create_bank_account(session: AsyncSession, user_id: str, account_data: BankAccountCreate) -> BankAccountInDB:
    account_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    CREATE (ba:BankAccount {
        id: $id,
        user_id: $user_id,
        bank_name: $bank_name,
        account_name: $account_name,
        account_id: $account_id,
        account_type: $account_type,
        currency: $currency,
        current_balance: toFloat($current_balance),
        is_synced: $is_synced,
        last_synced_at: datetime($last_synced_at),
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    RETURN ba
    """
    params = account_data.model_dump()
    params["id"] = account_neo4j_id
    params["user_id"] = user_id
    params["current_balance"] = float(params["current_balance"])
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()
    if params["last_synced_at"]:
        params["last_synced_at"] = params["last_synced_at"].isoformat()
    
    result = await session.run(query, params)
    record = await result.single()
    node = record["ba"]
    return BankAccountInDB(
        id=node["id"],
        user_id=node["user_id"],
        bank_name=node["bank_name"],
        account_name=node["account_name"],
        account_id=node["account_id"],
        account_type=node["account_type"],
        currency=node["currency"],
        current_balance=Decimal(str(node["current_balance"])),
        is_synced=node["is_synced"],
        last_synced_at=datetime.fromisoformat(node["last_synced_at"].iso_format()) if node["last_synced_at"] else None,
        created_at=datetime.fromisoformat(node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
    )

async def get_bank_account_by_id(session: AsyncSession, account_id: str, user_id: str) -> Optional[BankAccountInDB]:
    query = """
    MATCH (ba:BankAccount {account_id: $account_id, user_id: $user_id})
    RETURN ba
    """
    result = await session.run(query, account_id=account_id, user_id=user_id)
    record = await result.single()
    if record:
        node = record["ba"]
        return BankAccountInDB(
            id=node["id"],
            user_id=node["user_id"],
            bank_name=node["bank_name"],
            account_name=node["account_name"],
            account_id=node["account_id"],
            account_type=node["account_type"],
            currency=node["currency"],
            current_balance=Decimal(str(node["current_balance"])),
            is_synced=node["is_synced"],
            last_synced_at=datetime.fromisoformat(node["last_synced_at"].iso_format()) if node["last_synced_at"] else None,
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        )
    return None

async def get_all_bank_accounts(session: AsyncSession, user_id: str) -> List[BankAccountInDB]:
    query = """
    MATCH (ba:BankAccount {user_id: $user_id})
    RETURN ba
    ORDER BY ba.bank_name, ba.account_name
    """
    result = await session.run(query, user_id=user_id)
    accounts = []
    async for record in result:
        node = record["ba"]
        accounts.append(BankAccountInDB(
            id=node["id"],
            user_id=node["user_id"],
            bank_name=node["bank_name"],
            account_name=node["account_name"],
            account_id=node["account_id"],
            account_type=node["account_type"],
            currency=node["currency"],
            current_balance=Decimal(str(node["current_balance"])),
            is_synced=node["is_synced"],
            last_synced_at=datetime.fromisoformat(node["last_synced_at"].iso_format()) if node["last_synced_at"] else None,
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        ))
    return accounts

async def update_bank_account(session: AsyncSession, account_id: str, user_id: str, account_data: BankAccountUpdate) -> Optional[BankAccountInDB]:
    update_fields = account_data.model_dump(exclude_unset=True)
    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "current_balance" in update_fields:
        update_fields["current_balance"] = float(update_fields["current_balance"])
    if "last_synced_at" in update_fields and update_fields["last_synced_at"]:
        update_fields["last_synced_at"] = update_fields["last_synced_at"].isoformat()

    set_clauses = [f"ba.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (ba:BankAccount {{account_id: $account_id, user_id: $user_id}})
    SET {set_query_part}
    RETURN ba
    """
    
    params = {"account_id": account_id, "user_id": user_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        node = record["ba"]
        return BankAccountInDB(
            id=node["id"],
            user_id=node["user_id"],
            bank_name=node["bank_name"],
            account_name=node["account_name"],
            account_id=node["account_id"],
            account_type=node["account_type"],
            currency=node["currency"],
            current_balance=Decimal(str(node["current_balance"])),
            is_synced=node["is_synced"],
            last_synced_at=datetime.fromisoformat(node["last_synced_at"].iso_format()) if node["last_synced_at"] else None,
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        )
    return None

async def delete_bank_account(session: AsyncSession, account_id: str, user_id: str) -> bool:
    # Delete BankAccount node and all its transactions
    query = """
    MATCH (ba:BankAccount {account_id: $account_id, user_id: $user_id})
    OPTIONAL MATCH (ba)-[:HAS_TRANSACTION]->(bt:BankTransaction)
    DETACH DELETE ba, bt
    """
    result = await session.run(query, account_id=account_id, user_id=user_id)
    return result.consume().counters.nodes_deleted > 0

# --- BankTransaction CRUD ---
async def create_bank_transaction(session: AsyncSession, bank_account_neo4j_id: str, transaction_data: BankTransactionCreate) -> BankTransactionInDB:
    transaction_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    MATCH (ba:BankAccount {id: $bank_account_neo4j_id})
    CREATE (bt:BankTransaction {
        id: $id,
        transaction_id: $transaction_id,
        date: datetime($date),
        description: $description,
        amount: toFloat($amount),
        transaction_type: $transaction_type,
        category: $category,
        reconciled: $reconciled,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (ba)-[:HAS_TRANSACTION]->(bt)
    RETURN bt
    """
    params = transaction_data.model_dump()
    params["id"] = transaction_neo4j_id
    params["bank_account_neo4j_id"] = bank_account_neo4j_id
    params["amount"] = float(params["amount"])
    params["date"] = params["date"].isoformat()
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    node = record["bt"]
    return BankTransactionInDB(
        id=node["id"],
        bank_account_id=bank_account_neo4j_id,
        transaction_id=node["transaction_id"],
        date=datetime.fromisoformat(node["date"].iso_format()),
        description=node["description"],
        amount=Decimal(str(node["amount"])),
        transaction_type=node["transaction_type"],
        category=node["category"],
        reconciled=node["reconciled"],
        created_at=datetime.fromisoformat(node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
    )

async def get_bank_transactions_for_account(session: AsyncSession, bank_account_id: str, user_id: str) -> List[BankTransactionInDB]:
    query = """
    MATCH (ba:BankAccount {account_id: $bank_account_id, user_id: $user_id})-[:HAS_TRANSACTION]->(bt:BankTransaction)
    RETURN bt
    ORDER BY bt.date DESC
    """
    result = await session.run(query, bank_account_id=bank_account_id, user_id=user_id)
    transactions = []
    async for record in result:
        node = record["bt"]
        transactions.append(BankTransactionInDB(
            id=node["id"],
            bank_account_id=bank_account_id,
            transaction_id=node["transaction_id"],
            date=datetime.fromisoformat(node["date"].iso_format()),
            description=node["description"],
            amount=Decimal(str(node["amount"])),
            transaction_type=node["transaction_type"],
            category=node["category"],
            reconciled=node["reconciled"],
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        ))
    return transactions

async def get_bank_transaction_by_id(session: AsyncSession, transaction_id: str, user_id: str) -> Optional[BankTransactionInDB]:
    query = """
    MATCH (ba:BankAccount {user_id: $user_id})-[:HAS_TRANSACTION]->(bt:BankTransaction {transaction_id: $transaction_id})
    RETURN bt, ba.id as bank_account_neo4j_id
    """
    result = await session.run(query, transaction_id=transaction_id, user_id=user_id)
    record = await result.single()
    if record:
        node = record["bt"]
        return BankTransactionInDB(
            id=node["id"],
            bank_account_id=record["bank_account_neo4j_id"],
            transaction_id=node["transaction_id"],
            date=datetime.fromisoformat(node["date"].iso_format()),
            description=node["description"],
            amount=Decimal(str(node["amount"])),
            transaction_type=node["transaction_type"],
            category=node["category"],
            reconciled=node["reconciled"],
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        )
    return None

# This is a mock function for fetching transactions from an external API
async def mock_fetch_external_transactions(bank_account_id: str, user_id: str, count: int = 5) -> List[BankTransactionCreate]:
    transactions = []
    for i in range(count):
        amount = Decimal(f"{(-1)**i * (10.00 + i * 5)}.00") # Alternating debit/credit
        transactions.append(BankTransactionCreate(
            transaction_id=f"MOCK_TXN_{user_id}_{bank_account_id}_{datetime.now().strftime('%Y%m%d%H%M%S')}_{i}",
            date=datetime.now() - timedelta(days=i * 2),
            description=f"Mock Transaction {i} for {bank_account_id}",
            amount=amount,
            transaction_type="debit" if amount < 0 else "credit",
            category="Mock Category",
            reconciled=False
        ))
    return transactions
