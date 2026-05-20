from neo4j import AsyncSession
from typing import Optional, List
from accounting_service.models import AccountCreate, AccountUpdate, AccountInDB
from datetime import datetime
import uuid

async def create_account(session: AsyncSession, account_data: AccountCreate) -> AccountInDB:
    query = """
    CREATE (a:Account {
        id: $id,
        account_number: $account_number,
        account_name: $account_name,
        account_type: $account_type,
        normal_balance: $normal_balance,
        description: $description,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    WITH a
    OPTIONAL MATCH (parent:Account {account_number: $parent_account_number})
    WHERE $parent_account_number IS NOT NULL
    CREATE (a)-[:HAS_PARENT]->(parent)
    RETURN a
    """
    id_ = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    result = await session.run(
        query,
        id=id_,
        created_at=created_at.isoformat(),
        updated_at=updated_at.isoformat(),
        **account_data.model_dump()
    )
    record = await result.single()
    node = record["a"]
    return AccountInDB(
        id=node["id"],
        account_number=node["account_number"],
        account_name=node["account_name"],
        account_type=node["account_type"],
        normal_balance=node["normal_balance"],
        description=node["description"],
        parent_account_number=account_data.parent_account_number, # Neo4j doesn't return this directly from node on create
        created_at=datetime.fromisoformat(node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
    )

async def get_account_by_number(session: AsyncSession, account_number: str) -> Optional[AccountInDB]:
    query = """
    MATCH (a:Account {account_number: $account_number})
    OPTIONAL MATCH (a)-[:HAS_PARENT]->(parent:Account)
    RETURN a, parent.account_number AS parent_account_number
    """
    result = await session.run(query, account_number=account_number)
    record = await result.single()
    if record:
        node = record["a"]
        return AccountInDB(
            id=node["id"],
            account_number=node["account_number"],
            account_name=node["account_name"],
            account_type=node["account_type"],
            normal_balance=node["normal_balance"],
            description=node["description"],
            parent_account_number=record["parent_account_number"],
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        )
    return None

async def get_all_accounts(session: AsyncSession) -> List[AccountInDB]:
    query = """
    MATCH (a:Account)
    OPTIONAL MATCH (a)-[:HAS_PARENT]->(parent:Account)
    RETURN a, parent.account_number AS parent_account_number
    ORDER BY a.account_number
    """
    result = await session.run(query)
    accounts = []
    async for record in result:
        node = record["a"]
        accounts.append(AccountInDB(
            id=node["id"],
            account_number=node["account_number"],
            account_name=node["account_name"],
            account_type=node["account_type"],
            normal_balance=node["normal_balance"],
            description=node["description"],
            parent_account_number=record["parent_account_number"],
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        ))
    return accounts

async def update_account(session: AsyncSession, account_number: str, account_data: AccountUpdate) -> Optional[AccountInDB]:
    update_fields = {k: v for k, v in account_data.model_dump(exclude_unset=True).items() if k != "parent_account_number"}
    update_fields["updated_at"] = datetime.utcnow().isoformat()

    set_clauses = [f"a.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (a:Account {{account_number: $account_number}})
    SET {set_query_part}
    WITH a
    OPTIONAL MATCH (a)-[old_rel:HAS_PARENT]->()
    WHERE $parent_account_number IS NOT NULL AND old_rel IS NOT NULL
    DELETE old_rel
    WITH a
    OPTIONAL MATCH (parent:Account {{account_number: $parent_account_number}})
    WHERE $parent_account_number IS NOT NULL
    CREATE (a)-[:HAS_PARENT]->(parent)
    RETURN a, parent.account_number AS parent_account_number
    """
    
    params = {"account_number": account_number, **update_fields, "parent_account_number": account_data.parent_account_number}
    result = await session.run(query, params)
    record = await result.single()
    if record:
        node = record["a"]
        return AccountInDB(
            id=node["id"],
            account_number=node["account_number"],
            account_name=node["account_name"],
            account_type=node["account_type"],
            normal_balance=node["normal_balance"],
            description=node["description"],
            parent_account_number=record["parent_account_number"],
            created_at=datetime.fromisoformat(node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(node["updated_at"].iso_format()),
        )
    return None

async def delete_account(session: AsyncSession, account_number: str) -> bool:
    # Delete account and its relationships
    query = """
    MATCH (a:Account {account_number: $account_number})
    DETACH DELETE a
    """
    result = await session.run(query, account_number=account_number)
    return result.consume().counters.nodes_deleted > 0
