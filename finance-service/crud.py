from neo4j import AsyncSession
from typing import Optional, List
from finance_service.models import BudgetCreate, BudgetUpdate, BudgetInDB, BudgetItemCreate, BudgetItemInDB
from datetime import datetime
import uuid
from decimal import Decimal

async def create_budget(session: AsyncSession, budget_data: BudgetCreate) -> BudgetInDB:
    budget_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Create Budget node
    budget_query = """
    CREATE (b:Budget {
        id: $id,
        name: $name,
        fiscal_year: $fiscal_year,
        period: $period,
        description: $description,
        status: $status,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    RETURN b
    """
    budget_params = budget_data.model_dump(exclude={
        "items"
    })
    budget_result = await session.run(
        budget_query,
        id=budget_id,
        created_at=created_at.isoformat(),
        updated_at=updated_at.isoformat(),
        **budget_params
    )
    budget_node = (await budget_result.single())["b"]

    # Create BudgetItem nodes and link to Budget
    budget_items_in_db = []
    for item_data in budget_data.items:
        item_id = str(uuid.uuid4())
        item_created_at = datetime.utcnow()
        item_updated_at = datetime.utcnow()
        item_query = """
        MATCH (b:Budget {id: $budget_id})
        CREATE (bi:BudgetItem {
            id: $item_id,
            category: $category,
            budgeted_amount: toFloat($budgeted_amount),
            actual_amount: toFloat($actual_amount),
            description: $description,
            account_number: $account_number,
            period_start: datetime($period_start),
            period_end: datetime($period_end),
            created_at: datetime($created_at),
            updated_at: datetime($updated_at)
        })
        CREATE (b)-[:HAS_ITEM]->(bi)
        RETURN bi
        """
        item_params = item_data.model_dump()
        item_result = await session.run(
            item_query,
            budget_id=budget_id,
            item_id=item_id,
            created_at=item_created_at.isoformat(),
            updated_at=item_updated_at.isoformat(),
            budgeted_amount=float(item_params['budgeted_amount']),
            actual_amount=float(item_params['actual_amount']),
            period_start=item_params['period_start'].isoformat(),
            period_end=item_params['period_end'].isoformat(),
            **{k: v for k, v in item_params.items() if k not in ['budgeted_amount', 'actual_amount', 'period_start', 'period_end']}
        )
        item_node = (await item_result.single())["bi"]
        budget_items_in_db.append(BudgetItemInDB(
            id=item_node["id"],
            category=item_node["category"],
            budgeted_amount=Decimal(str(item_node["budgeted_amount"])),
            actual_amount=Decimal(str(item_node["actual_amount"])),
            description=item_node["description"],
            account_number=item_node["account_number"],
            period_start=datetime.fromisoformat(item_node["period_start"].iso_format()),
            period_end=datetime.fromisoformat(item_node["period_end"].iso_format()),
            created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
        ))
    
    return BudgetInDB(
        id=budget_node["id"],
        name=budget_node["name"],
        fiscal_year=budget_node["fiscal_year"],
        period=budget_node["period"],
        description=budget_node["description"],
        status=budget_node["status"],
        created_at=datetime.fromisoformat(budget_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(budget_node["updated_at"].iso_format()),
        items=budget_items_in_db
    )

async def get_budget(session: AsyncSession, budget_id: str) -> Optional[BudgetInDB]:
    query = """
    MATCH (b:Budget {id: $budget_id})-[:HAS_ITEM]->(bi:BudgetItem)
    RETURN b, COLLECT(bi) AS items
    """
    result = await session.run(query, budget_id=budget_id)
    record = await result.single()

    if record:
        budget_node = record["b"]
        items_data = record["items"]

        budget_items_in_db = []
        for item_node in items_data:
            budget_items_in_db.append(BudgetItemInDB(
                id=item_node["id"],
                category=item_node["category"],
                budgeted_amount=Decimal(str(item_node["budgeted_amount"])),
                actual_amount=Decimal(str(item_node["actual_amount"])),
                description=item_node["description"],
                account_number=item_node["account_number"],
                period_start=datetime.fromisoformat(item_node["period_start"].iso_format()),
                period_end=datetime.fromisoformat(item_node["period_end"].iso_format()),
                created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
            ))
        
        return BudgetInDB(
            id=budget_node["id"],
            name=budget_node["name"],
            fiscal_year=budget_node["fiscal_year"],
            period=budget_node["period"],
            description=budget_node["description"],
            status=budget_node["status"],
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
    ORDER BY b.fiscal_year DESC, b.period
    """
    result = await session.run(query)

    budgets = []
    # Group items by budget
    budgets_map = {}

    async for record in result:
        budget_node = record["b"]
        items_data = record["items"]
        budget_id = budget_node["id"]

        if budget_id not in budgets_map:
            budgets_map[budget_id] = BudgetInDB(
                id=budget_node["id"],
                name=budget_node["name"],
                fiscal_year=budget_node["fiscal_year"],
                period=budget_node["period"],
                description=budget_node["description"],
                status=budget_node["status"],
                created_at=datetime.fromisoformat(budget_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(budget_node["updated_at"].iso_format()),
                items=[] # Initialize empty list for items
            )
        
        for item_node in items_data:
            if item_node: # Only add if item_node is not None (for budgets with no items)
                budgets_map[budget_id].items.append(BudgetItemInDB(
                    id=item_node["id"],
                    category=item_node["category"],
                    budgeted_amount=Decimal(str(item_node["budgeted_amount"])),
                    actual_amount=Decimal(str(item_node["actual_amount"])),
                    description=item_node["description"],
                    account_number=item_node["account_number"],
                    period_start=datetime.fromisoformat(item_node["period_start"].iso_format()),
                    period_end=datetime.fromisoformat(item_node["period_end"].iso_format()),
                    created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                    updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
                ))
    
    budgets = list(budgets_map.values())
    return budgets


async def update_budget(session: AsyncSession, budget_id: str, budget_data: BudgetUpdate) -> Optional[BudgetInDB]:
    update_fields = budget_data.model_dump(exclude_unset=True, exclude={
        "items"
    })
    update_fields["updated_at"] = datetime.utcnow().isoformat()

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
        budget_node = record["b"]
        # For simplicity, items are not updated directly via this endpoint.
        # A separate endpoint for BudgetItem CRUD would be needed.
        return await get_budget(session, budget_id) # Fetch full budget with items
    return None

async def delete_budget(session: AsyncSession, budget_id: str) -> bool:
    # Delete Budget node and all associated BudgetItem nodes and relationships
    query = """
    MATCH (b:Budget {id: $budget_id})
    OPTIONAL MATCH (b)-[:HAS_ITEM]->(bi:BudgetItem)
    DETACH DELETE b, bi
    """
    result = await session.run(query, budget_id=budget_id)
    return result.consume().counters.nodes_deleted > 0
