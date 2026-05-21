from neo4j import AsyncSession
from typing import Optional, List, Dict, Any, Tuple
from finance_service.models import (
    BudgetCreate, BudgetUpdate, BudgetInDB, BudgetItemCreate, BudgetItemInDB, BudgetItemUpdate,
    ActualsSummary, BudgetItemBase, BudgetVarianceItem, BudgetVarianceReport,
    LiquidityRatios, SolvencyRatios, ProfitabilityRatios, FinancialRatiosReport,
    EfficiencyRatios, MarketRatios
)
from datetime import datetime, timedelta
import uuid
from decimal import Decimal
import httpx
import os
from finance_service.exceptions import ValidationError, NotFoundError

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")
ACCOUNTING_SERVICE_URL = f"{API_GATEWAY_URL}/accounting"

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


# --- Budget Item CRUD ---
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
        return await get_budget_item(session, budget_id, item_id) # No fields to update

    update_fields["updated_at"] = datetime.utcnow().isoformat()
    if "budgeted_amount" in update_fields and update_fields["budgeted_amount"]:
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


# --- NEW: Budget Variance Reporting ---
async def get_account_actual_balance_from_accounting_service(
    user_id: str,
    account_number: str,
    start_date: datetime,
    end_date: datetime,
    jwt_token: str # JWT token for authentication with internal services
) -> Tuple[Decimal, Decimal]:
    """
    Fetches the total debits and credits for a given account within a date range from the Accounting Service.
    """
    accounting_service_period_activity_url = f"{ACCOUNTING_SERVICE_URL}/accounts/{account_number}/period-activity"
    params = {
        "start_date": start_date.isoformat(),
        "end_date": end_date.isoformat(),
        "user_id": user_id
    }
    headers = {"Authorization": f"Bearer {jwt_token}"}

    try:
        async with httpx.AsyncClient() as client:
            response = await client.get(accounting_service_period_activity_url, params=params, headers=headers)
            response.raise_for_status() # Raise an exception for 4xx or 5xx status codes
            data = response.json()
            
            total_debits = Decimal(str(data.get("total_debits", 0.0)))
            total_credits = Decimal(str(data.get("total_credits", 0.0)))
            
            return total_debits, total_credits
            
    except httpx.HTTPStatusError as e:
        if e.response.status_code == 404:
            print(f"Account {account_number} not found in accounting service for period activity.")
            return Decimal('0.00'), Decimal('0.00')
        raise ValidationError(detail=f"Error fetching actuals from accounting service: {e.response.text}")
    except httpx.RequestError as e:
        raise ValidationError(detail=f"Network error fetching actuals from accounting service: {e}")
    except Exception as e:
        raise ValidationError(detail=f"Unexpected error fetching actuals from accounting service: {e}")


async def get_budget_variance_report(
    session: AsyncSession,
    user_id: str,
    budget_id: str,
    jwt_token: str # Required to call accounting service
) -> BudgetVarianceReport:
    """
    Generates a budget vs. actuals variance report for a given budget.
    """
    budget = await get_budget(session, budget_id)
    if not budget:
        raise NotFoundError(detail=f"Budget with ID {budget_id} not found.")

    variance_items: List[BudgetVarianceItem] = []
    total_budgeted_expense = Decimal('0.00')
    total_actual_expense = Decimal('0.00')
    total_budgeted_revenue = Decimal('0.00')
    total_actual_revenue = Decimal('0.00')

    for item in budget.items:
        # Fetch actual activity for the account associated with the budget item
        actual_debits, actual_credits = await get_account_actual_balance_from_accounting_service(
            user_id=user_id,
            account_number=item.account_number,
            start_date=budget.start_date,
            end_date=budget.end_date,
            jwt_token=jwt_token
        )

        actual_amount = Decimal('0.00')
        if item.budget_type == 'expense':
            actual_amount = actual_debits # Expenses increase with debits
            total_budgeted_expense += item.budgeted_amount
            total_actual_expense += actual_amount
        elif item.budget_type == 'revenue':
            actual_amount = actual_credits # Revenues increase with credits
            total_budgeted_revenue += item.budgeted_amount
            total_actual_revenue += actual_amount
        
        variance = actual_amount - item.budgeted_amount
        variance_percentage = (variance / item.budgeted_amount * Decimal('100')) if item.budgeted_amount != Decimal('0.00') else Decimal('0.00')

        variance_items.append(
            BudgetVarianceItem(
                category=item.category,
                account_number=item.account_number,
                budgeted_amount=item.budgeted_amount,
                actual_amount=actual_amount,
                variance=variance,
                variance_percentage=variance_percentage,
                budget_type=item.budget_type
            )
        )
    
    overall_variance_expense = total_actual_expense - total_budgeted_expense
    overall_variance_revenue = total_actual_revenue - total_budgeted_revenue

    return BudgetVarianceReport(
        budget_id=budget.id,
        budget_name=budget.name,
        start_date=budget.start_date,
        end_date=budget.end_date,
        currency=budget.currency,
        items=variance_items,
        total_budgeted_expense=total_budgeted_expense,
        total_actual_expense=total_actual_expense,
        overall_variance_expense=overall_variance_expense,
        total_budgeted_revenue=total_budgeted_revenue,
        total_actual_revenue=total_actual_revenue,
        overall_variance_revenue=overall_variance_revenue,
    )
