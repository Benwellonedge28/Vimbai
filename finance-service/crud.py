from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from finance_service.models import (
    BudgetCreate, BudgetUpdate, BudgetInDB, BudgetItemCreate, BudgetItemInDB, BudgetItemUpdate, # Added BudgetItemUpdate
    ActualsSummary, BudgetVarianceItem, BudgetVarianceReport,
    LiquidityRatios, SolvencyRatios, ProfitabilityRatios, FinancialRatiosReport,
    EfficiencyRatios, MarketRatios
)
from datetime import datetime
import uuid
from decimal import Decimal
import httpx
import os
from finance_service.exceptions import ValidationError, NotFoundError

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# --- Budget CRUD (modified to handle items separately) ---
async def create_budget(session: AsyncSession, budget_data: BudgetCreate) -> BudgetInDB:
    budget_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    query = """
    CREATE (b:Budget {
        id: $id,
        name: $name,
        start_date: datetime($start_date),
        end_date: datetime($end_date),
        currency: $currency,
        description: $description,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    RETURN b
    """
    params = budget_data.model_dump()
    params["id"] = budget_id
    params["start_date"] = params["start_date"].isoformat()
    params["end_date"] = params["end_date"].isoformat()
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    budget_node = record["b"]

    return BudgetInDB(
        id=budget_node["id"],
        name=budget_node["name"],
        start_date=datetime.fromisoformat(budget_node["start_date"].iso_format()),
        end_date=datetime.fromisoformat(budget_node["end_date"].iso_format()),
        currency=budget_node["currency"],
        description=budget_node["description"],
        created_at=datetime.fromisoformat(budget_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(budget_node["updated_at"].iso_format()),
        items=[] # No items on initial creation
    )

async def get_budget(session: AsyncSession, budget_id: str) -> Optional[BudgetInDB]:
    query = """
    MATCH (b:Budget {id: $budget_id})
    OPTIONAL MATCH (b)-[:HAS_ITEM]->(bi:BudgetItem)
    RETURN b, COLLECT(bi) AS items
    """
    result = await session.run(query, budget_id=budget_id)
    record = await result.single()

    if record:
        budget_node = record["b"]
        items_data = record["items"]
        
        budget_items_in_db = []
        for item_node in items_data:
            if item_node: # Only add if item_node is not None (COLLECT can return [None] if no items)
                budget_items_in_db.append(BudgetItemInDB(
                    id=item_node["id"],
                    budget_id=budget_id,
                    category=item_node["category"],
                    account_number=item_node["account_number"],
                    budgeted_amount=Decimal(str(item_node["budgeted_amount"])),
                    budget_type=item_node["budget_type"],
                    created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                    updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
                ))
        
        return BudgetInDB(
            id=budget_node["id"],
            name=budget_node["name"],
            start_date=datetime.fromisoformat(budget_node["start_date"].iso_format()),
            end_date=datetime.fromisoformat(budget_node["end_date"].iso_format()),
            currency=budget_node["currency"],
            description=budget_node["description"],
            created_at=datetime.fromisoformat(budget_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(budget_node["updated_at"].iso_format()),
            items=budget_items_in_db
        )
    return None

async def get_all_budgets(session: AsyncSession) -> List[BudgetInDB]:
    query = """
    MATCH (b:Budget)
    OPTIONAL MATCH (b)-[:HAS_ITEM]->(bi:BudgetItem)
    RETURN b, COLLECT(bi) AS items
    ORDER BY b.start_date DESC
    """
    result = await session.run(query)

    budgets_map: Dict[str, BudgetInDB] = {}

    async for record in result:
        budget_node = record["b"]
        items_data = record["items"]
        budget_id = budget_node["id"]

        if budget_id not in budgets_map:
            budgets_map[budget_id] = BudgetInDB(
                id=budget_node["id"],
                name=budget_node["name"],
                start_date=datetime.fromisoformat(budget_node["start_date"].iso_format()),
                end_date=datetime.fromisoformat(budget_node["end_date"].iso_format()),
                currency=budget_node["currency"],
                description=budget_node["description"],
                created_at=datetime.fromisoformat(budget_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(budget_node["updated_at"].iso_format()),
                items=[],
            )
        
        for item_node in items_data:
            if item_node:
                budgets_map[budget_id].items.append(BudgetItemInDB(
                    id=item_node["id"],
                    budget_id=budget_id,
                    category=item_node["category"],
                    account_number=item_node["account_number"],
                    budgeted_amount=Decimal(str(item_node["budgeted_amount"])),
                    budget_type=item_node["budget_type"],
                    created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                    updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
                ))
    
    return list(budgets_map.values())
    
async def update_budget(session: AsyncSession, budget_id: str, budget_data: BudgetUpdate) -> Optional[BudgetInDB]:
    update_fields = budget_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_budget(session, budget_id) # No fields to update

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "start_date" in update_fields and update_fields["start_date"]:
        update_fields["start_date"] = update_fields["start_date"].isoformat()
    if "end_date" in update_fields and update_fields["end_date"]:
        update_fields["end_date"] = update_fields["end_date"].isoformat()

    set_clauses = [f"b.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (b:Budget {{id: $budget_id}})
    SET {set_query_part}
    RETURN b
    """
    
    params = {"budget_id": budget_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_budget(session, budget_id)
    return None

async def delete_budget(session: AsyncSession, budget_id: str) -> bool:
    query = """
    MATCH (b:Budget {id: $budget_id})
    OPTIONAL MATCH (b)-[:HAS_ITEM]->(bi:BudgetItem)
    DETACH DELETE b, bi
    """
    result = await session.run(query, budget_id=budget_id)
    return result.consume().counters.nodes_deleted > 0


# --- Budget Item CRUD (NEW) ---
async def create_budget_item(session: AsyncSession, budget_id: str, item_data: BudgetItemCreate) -> Optional[BudgetItemInDB]:
    item_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Check if the account number exists in the Accounting Service
    # This interaction implies a call to the Accounting Service's COA endpoint
    # For simplicity, we assume account validation is done upstream or the account exists.
    
    query = """
    MATCH (b:Budget {id: $budget_id})
    CREATE (bi:BudgetItem {
        id: $id,
        category: $category,
        account_number: $account_number,
        budgeted_amount: toFloat($budgeted_amount),
        budget_type: $budget_type,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (b)-[:HAS_ITEM]->(bi)
    RETURN bi
    """
    params = item_data.model_dump()
    params["id"] = item_id
    params["budget_id"] = budget_id
    params["budgeted_amount"] = float(params["budgeted_amount"])
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    if record:
        item_node = record["bi"]
        return BudgetItemInDB(
            id=item_node["id"],
            budget_id=budget_id,
            category=item_node["category"],
            account_number=item_node["account_number"],
            budgeted_amount=Decimal(str(item_node["budgeted_amount"])),
            budget_type=item_node["budget_type"],
            created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
        )
    return None

async def get_budget_item(session: AsyncSession, budget_id: str, item_id: str) -> Optional[BudgetItemInDB]:
    query = """
    MATCH (b:Budget {id: $budget_id})-[:HAS_ITEM]->(bi:BudgetItem {id: $item_id})
    RETURN bi
    """
    result = await session.run(query, budget_id=budget_id, item_id=item_id)
    record = await result.single()

    if record:
        item_node = record["bi"]
        return BudgetItemInDB(
            id=item_node["id"],
            budget_id=budget_id,
            category=item_node["category"],
            account_number=item_node["account_number"],
            budgeted_amount=Decimal(str(item_node["budgeted_amount"])),
            budget_type=item_node["budget_type"],
            created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
        )
    return None

async def update_budget_item(session: AsyncSession, budget_id: str, item_id: str, item_data: BudgetItemUpdate) -> Optional[BudgetItemInDB]:
    update_fields = item_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_budget_item(session, budget_id, item_id)

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "budgeted_amount" in update_fields:
        update_fields["budgeted_amount"] = float(update_fields["budgeted_amount"])

    set_clauses = [f"bi.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (b:Budget {{id: $budget_id}})-[:HAS_ITEM]->(bi:BudgetItem {{id: $item_id}})
    SET {set_query_part}
    RETURN bi
    """
    
    params = {"budget_id": budget_id, "item_id": item_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_budget_item(session, budget_id, item_id)
    return None

async def delete_budget_item(session: AsyncSession, budget_id: str, item_id: str) -> bool:
    query = """
    MATCH (b:Budget {id: $budget_id})-[:HAS_ITEM]->(bi:BudgetItem {id: $item_id})
    DETACH DELETE bi
    """
    result = await session.run(query, budget_id=budget_id, item_id=item_id)
    return result.consume().counters.nodes_deleted > 0

# --- Accounting Service Data Fetching (unchanged) ---
async def fetch_income_statement_from_accounting(jwt_token: str, start_date: datetime, end_date: datetime) -> Dict[str, Any]:
    # ... (unchanged) ...
    pass
async def fetch_balance_sheet_from_accounting(jwt_token: str, as_of_date: datetime) -> Dict[str, Any]:
    # ... (unchanged) ...
    pass

# --- Financial Ratio Calculations (unchanged) ---
async def generate_financial_ratios_report(jwt_token: str, start_date: datetime, end_date: datetime) -> FinancialRatiosReport:
    # ... (unchanged) ...
    pass
