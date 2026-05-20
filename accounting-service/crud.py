from neo4j import AsyncSession
from typing import Optional, List
from accounting_service.models import AccountCreate, AccountUpdate, AccountInDB, JournalEntryCreate, JournalEntryInDB # NEW imports
from datetime import datetime
import uuid
from decimal import Decimal

# --- Account CRUD ---

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


# --- Journal Entry CRUD (NEW) ---

async def create_journal_entry(session: AsyncSession, entry_data: JournalEntryCreate) -> JournalEntryInDB:
    entry_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Cypher to create JournalEntry node and its lines, linking to Accounts
    query = f"""
    CREATE (je:JournalEntry {{
        id: $id,
        entry_date: datetime($entry_date),
        description: $description,
        reference_number: $reference_number,
        source_module: $source_module,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    }})
    WITH je
    UNWIND $lines AS line
    MATCH (a:Account {{account_number: line.account_number}})
    CREATE (jl:JournalLine {{
        id: toString(randomUUID()),
        debit: toFloat(line.debit),
        credit: toFloat(line.credit),
        description: line.description
    }})
    CREATE (je)-[:HAS_LINE]->(jl)
    CREATE (jl)-[:AFFECTS]->(a)
    RETURN je, collect(jl) as lines, collect(a) as accounts
    """
    
    # Convert Decimal to float for Neo4j (or handle as string if precision is paramount with driver settings)
    # For now, FastAPI's condecimal ensures it's a Decimal, convert to float for storage.
    lines_for_neo4j = []
    for line in entry_data.lines:
        lines_for_neo4j.append({
            "account_number": line.account_number,
            "debit": float(line.debit),
            "credit": float(line.credit),
            "description": line.description
        })

    result = await session.run(
        query,
        id=entry_id,
        entry_date=entry_data.entry_date.isoformat(),
        description=entry_data.description,
        reference_number=entry_data.reference_number,
        source_module=entry_data.source_module,
        created_at=created_at.isoformat(),
        updated_at=updated_at.isoformat(),
        lines=lines_for_neo4j
    )

    record = await result.single()
    je_node = record["je"]
    
    # Reconstruct the JournalEntryInDB model from the returned data
    reconstructed_lines = []
    # Note: Retrieving line details and affected accounts in a single query would be more efficient
    # For now, assuming successful creation, this reconstructs based on input
    for line_data in entry_data.lines:
        reconstructed_lines.append(models.JournalLineBase(
            account_number=line_data.account_number,
            debit=line_data.debit,
            credit=line_data.credit,
            description=line_data.description
        ))

    return JournalEntryInDB(
        id=je_node["id"],
        entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
        description=je_node["description"],
        reference_number=je_node["reference_number"],
        source_module=je_node["source_module"],
        created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
        lines=reconstructed_lines
    )

async def get_journal_entry(session: AsyncSession, entry_id: str) -> Optional[JournalEntryInDB]:
    query = """
    MATCH (je:JournalEntry {id: $entry_id})-[:HAS_LINE]->(jl:JournalLine)-[:AFFECTS]->(a:Account)
    RETURN je, collect({
        id: jl.id,
        account_number: a.account_number,
        debit: jl.debit,
        credit: jl.credit,
        description: jl.description
    }) AS lines
    """
    result = await session.run(query, entry_id=entry_id)
    record = await result.single()

    if record:
        je_node = record["je"]
        lines_data = record["lines"]
        
        reconstructed_lines = []
        for line_data in lines_data:
            reconstructed_lines.append(models.JournalLineBase(
                account_number=line_data["account_number"],
                debit=Decimal(str(line_data["debit"])), # Convert float back to Decimal
                credit=Decimal(str(line_data["credit"])),
                description=line_data["description"]
            ))

        return JournalEntryInDB(
            id=je_node["id"],
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            lines=reconstructed_lines
        )
    return None

async def get_all_journal_entries(session: AsyncSession) -> List[JournalEntryInDB]:
    query = """
    MATCH (je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:AFFECTS]->(a:Account)
    RETURN je, collect({
        id: jl.id,
        account_number: a.account_number,
        debit: jl.debit,
        credit: jl.credit,
        description: jl.description
    }) AS lines
    ORDER BY je.entry_date DESC
    """
    result = await session.run(query)
    entries = []
    async for record in result:
        je_node = record["je"]
        lines_data = record["lines"]

        reconstructed_lines = []
        for line_data in lines_data:
            reconstructed_lines.append(models.JournalLineBase(
                account_number=line_data["account_number"],
                debit=Decimal(str(line_data["debit"])),
                credit=Decimal(str(line_data["credit"])),
                description=line_data["description"]
            ))
        
        entries.append(JournalEntryInDB(
            id=je_node["id"],
            entry_date=datetime.fromisoformat(je_node["entry_date"].iso_format()),
            description=je_node["description"],
            reference_number=je_node["reference_number"],
            source_module=je_node["source_module"],
            created_at=datetime.fromisoformat(je_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(je_node["updated_at"].iso_format()),
            lines=reconstructed_lines
        ))
    return entries

async def delete_journal_entry(session: AsyncSession, entry_id: str) -> bool:
    # Delete JournalEntry node and all associated JournalLine nodes and relationships
    query = """
    MATCH (je:JournalEntry {id: $entry_id})
    OPTIONAL MATCH (je)-[:HAS_LINE]->(jl:JournalLine)
    DETACH DELETE je, jl
    """
    result = await session.run(query, entry_id=entry_id)
    # Check if at least one JournalEntry node was deleted
    return result.consume().counters.nodes_deleted > 0

# Note: Update of Journal Entries is highly complex in accounting, often leading to
# the creation of adjusting or reversing entries rather than direct modification.
# For simplicity, we won't implement a direct `update_journal_entry` that modifies lines directly.
# If required, it would involve deleting existing lines and creating new ones,
# or creating reversal entries.
