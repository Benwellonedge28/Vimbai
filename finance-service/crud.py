from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from finance_service.models import (
    BudgetItemCreate, BudgetItemUpdate, BudgetItemInDB,
    BudgetCreate, BudgetUpdate, BudgetInDB,
    FinancialForecastCreate, FinancialForecastUpdate, FinancialForecastInDB, # NEW
    ScenarioCreate, ScenarioUpdate, ScenarioInDB, # NEW
    FinancialForecastDataPoint, ScenarioParameter # NEW
)
from datetime import datetime, date, timezone
import uuid

# --- Budget CRUD ---
async def create_budget(session: AsyncSession, budget_data: BudgetCreate, user_id: str) -> BudgetInDB:
    budget_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (b:Budget {
        id: $id,
        name: $name,
        start_date: date($start_date),
        end_date: date($end_date),
        currency: $currency,
        description: $description,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_BUDGET]->(b)
    RETURN b
    """
    params = budget_data.model_dump()
    params["id"] = budget_id
    params["user_id"] = user_id
    params["start_date"] = params["start_date"].isoformat().split('T')[0] # Store as date string
    params["end_date"] = params["end_date"].isoformat().split('T')[0] # Store as date string
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    b_node = record["b"]

    return BudgetInDB(
        id=b_node["id"],
        name=b_node["name"],
        start_date=datetime.fromisoformat(b_node["start_date"]),
        end_date=datetime.fromisoformat(b_node["end_date"]),
        currency=b_node["currency"],
        description=b_node["description"],
        created_at=datetime.fromisoformat(b_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(b_node["updated_at"].iso_format()),
        items=[]
    )

async def get_budget(session: AsyncSession, budget_id: str) -> Optional[BudgetInDB]:
    query = """
    MATCH (b:Budget {id: $budget_id})
    OPTIONAL MATCH (b)-[:HAS_ITEM]->(bi:BudgetItem)
    RETURN b, COLLECT(bi) AS budget_items
    """
    result = await session.run(query, budget_id=budget_id)
    record = await result.single()

    if record:
        b_node = record["b"]
        budget_items = []
        for bi_node in record["budget_items"]:
            budget_items.append(BudgetItemInDB(
                id=bi_node["id"],
                budget_id=bi_node["budget_id"],
                category=bi_node["category"],
                account_number=bi_node["account_number"],
                budgeted_amount=bi_node["budgeted_amount"],
                budget_type=bi_node["budget_type"],
                created_at=datetime.fromisoformat(bi_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(bi_node["updated_at"].iso_format()),
            ))
        
        return BudgetInDB(
            id=b_node["id"],
            name=b_node["name"],
            start_date=datetime.fromisoformat(b_node["start_date"]),
            end_date=datetime.fromisoformat(b_node["end_date"]),
            currency=b_node["currency"],
            description=b_node["description"],
            created_at=datetime.fromisoformat(b_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(b_node["updated_at"].iso_format()),
            items=budget_items
        )
    return None

async def get_budgets_by_user(session: AsyncSession, user_id: str) -> List[BudgetInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BUDGET]->(b:Budget)
    OPTIONAL MATCH (b)-[:HAS_ITEM]->(bi:BudgetItem)
    RETURN b, COLLECT(bi) AS budget_items
    ORDER BY b.start_date DESC
    """
    result = await session.run(query, user_id=user_id)
    budgets = []
    async for record in result:
        b_node = record["b"]
        budget_items = []
        for bi_node in record["budget_items"]:
            budget_items.append(BudgetItemInDB(
                id=bi_node["id"],
                budget_id=bi_node["budget_id"],
                category=bi_node["category"],
                account_number=bi_node["account_number"],
                budgeted_amount=bi_node["budgeted_amount"],
                budget_type=bi_node["budget_type"],
                created_at=datetime.fromisoformat(bi_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(bi_node["updated_at"].iso_format()),
            ))
        budgets.append(BudgetInDB(
            id=b_node["id"],
            name=b_node["name"],
            start_date=datetime.fromisoformat(b_node["start_date"]),
            end_date=datetime.fromisoformat(b_node["end_date"]),
            currency=b_node["currency"],
            description=b_node["description"],
            created_at=datetime.fromisoformat(b_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(b_node["updated_at"].iso_format()),
            items=budget_items
        ))
    return budgets

async def update_budget(session: AsyncSession, budget_id: str, budget_data: BudgetUpdate) -> Optional[BudgetInDB]:
    update_fields = budget_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_budget(session, budget_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "start_date" in update_fields:
        update_fields["start_date"] = update_fields["start_date"].isoformat().split('T')[0]
    if "end_date" in update_fields:
        update_fields["end_date"] = update_fields["end_date"].isoformat().split('T')[0]

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
    DETACH DELETE b
    """
    result = await session.run(query, budget_id=budget_id)
    return result.consume().counters.nodes_deleted > 0

# --- BudgetItem CRUD ---
async def create_budget_item(session: AsyncSession, budget_id: str, item_data: BudgetItemCreate) -> BudgetItemInDB:
    item_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    query = """
    MATCH (b:Budget {id: $budget_id})
    CREATE (bi:BudgetItem {
        id: $id,
        budget_id: $budget_id,
        category: $category,
        account_number: $account_number,
        budgeted_amount: $budgeted_amount,
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
    params["budgeted_amount"] = float(params["budgeted_amount"]) # Convert Decimal to float for Neo4j
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    bi_node = record["bi"]

    return BudgetItemInDB(
        id=bi_node["id"],
        budget_id=bi_node["budget_id"],
        category=bi_node["category"],
        account_number=bi_node["account_number"],
        budgeted_amount=bi_node["budgeted_amount"],
        budget_type=bi_node["budget_type"],
        created_at=datetime.fromisoformat(bi_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(bi_node["updated_at"].iso_format()),
    )

async def update_budget_item(session: AsyncSession, item_id: str, item_data: BudgetItemUpdate) -> Optional[BudgetItemInDB]:
    update_fields = item_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_budget_item(session, item_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "budgeted_amount" in update_fields:
        update_fields["budgeted_amount"] = float(update_fields["budgeted_amount"])

    set_clauses = [f"bi.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (bi:BudgetItem {{id: $item_id}})
    SET {set_query_part}
    RETURN bi
    """
    params = {"item_id": item_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        # Re-fetch to get parent budget_id
        fetch_query = """
        MATCH (b:Budget)-[:HAS_ITEM]->(bi:BudgetItem {id: $item_id})
        RETURN b, bi
        """
        fetch_result = await session.run(fetch_query, item_id=item_id)
        fetch_record = await fetch_result.single()
        if fetch_record:
            b_node = fetch_record["b"]
            bi_node = fetch_record["bi"]
            return BudgetItemInDB(
                id=bi_node["id"],
                budget_id=b_node["id"],
                category=bi_node["category"],
                account_number=bi_node["account_number"],
                budgeted_amount=bi_node["budgeted_amount"],
                budget_type=bi_node["budget_type"],
                created_at=datetime.fromisoformat(bi_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(bi_node["updated_at"].iso_format()),
            )
    return None

async def get_budget_item(session: AsyncSession, item_id: str) -> Optional[BudgetItemInDB]:
    query = """
    MATCH (b:Budget)-[:HAS_ITEM]->(bi:BudgetItem {id: $item_id})
    RETURN b, bi
    """
    result = await session.run(query, item_id=item_id)
    record = await result.single()

    if record:
        b_node = record["b"]
        bi_node = record["bi"]
        return BudgetItemInDB(
            id=bi_node["id"],
            budget_id=b_node["id"],
            category=bi_node["category"],
            account_number=bi_node["account_number"],
            budgeted_amount=bi_node["budgeted_amount"],
            budget_type=bi_node["budget_type"],
            created_at=datetime.fromisoformat(bi_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(bi_node["updated_at"].iso_format()),
        )
    return None

async def delete_budget_item(session: AsyncSession, item_id: str) -> bool:
    query = """
    MATCH (bi:BudgetItem {id: $item_id})
    DETACH DELETE bi
    """
    result = await session.run(query, item_id=item_id)
    return result.consume().counters.nodes_deleted > 0

# --- FinancialForecast CRUD (NEW) ---
async def create_financial_forecast(session: AsyncSession, forecast_data: FinancialForecastCreate) -> FinancialForecastInDB:
    forecast_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Convert data points to dictionaries for storage
    data_points_data = [dp.model_dump() for dp in forecast_data.data_points]
    for dp in data_points_data:
        dp["period"] = dp["period"].isoformat()
        dp["amount"] = float(dp["amount"])

    query = """
    MATCH (u:User {id: $owner_user_id})
    CREATE (ff:FinancialForecast {
        id: $id,
        name: $name,
        description: $description,
        start_date: date($start_date),
        end_date: date($end_date),
        forecast_type: $forecast_type,
        methodology: $methodology,
        owner_user_id: $owner_user_id,
        data_points: $data_points,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_FORECAST]->(ff)
    RETURN ff
    """
    params = forecast_data.model_dump()
    params["id"] = forecast_id
    params["start_date"] = params["start_date"].isoformat()
    params["end_date"] = params["end_date"].isoformat()
    params["data_points"] = data_points_data
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    ff_node = record["ff"]

    reconstructed_data_points = []
    if ff_node.get("data_points"):
        for dp in ff_node["data_points"]:
            reconstructed_data_points.append(FinancialForecastDataPoint(
                period=date.fromisoformat(dp["period"]),
                value_type=dp["value_type"],
                amount=Decimal(str(dp["amount"]))
            ))

    return FinancialForecastInDB(
        id=ff_node["id"],
        name=ff_node["name"],
        description=ff_node["description"],
        start_date=date.fromisoformat(ff_node["start_date"]),
        end_date=date.fromisoformat(ff_node["end_date"]),
        forecast_type=ff_node["forecast_type"],
        methodology=ff_node["methodology"],
        owner_user_id=ff_node["owner_user_id"],
        data_points=reconstructed_data_points,
        created_at=datetime.fromisoformat(ff_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(ff_node["updated_at"].iso_format()),
    )

async def get_financial_forecast(session: AsyncSession, forecast_id: str) -> Optional[FinancialForecastInDB]:
    query = """
    MATCH (ff:FinancialForecast {id: $forecast_id})
    RETURN ff
    """
    result = await session.run(query, forecast_id=forecast_id)
    record = await result.single()

    if record:
        ff_node = record["ff"]
        reconstructed_data_points = []
        if ff_node.get("data_points"):
            for dp in ff_node["data_points"]:
                reconstructed_data_points.append(FinancialForecastDataPoint(
                    period=date.fromisoformat(dp["period"]),
                    value_type=dp["value_type"],
                    amount=Decimal(str(dp["amount"]))
                ))
        return FinancialForecastInDB(
            id=ff_node["id"],
            name=ff_node["name"],
            description=ff_node["description"],
            start_date=date.fromisoformat(ff_node["start_date"]),
            end_date=date.fromisoformat(ff_node["end_date"]),
            forecast_type=ff_node["forecast_type"],
            methodology=ff_node["methodology"],
            owner_user_id=ff_node["owner_user_id"],
            data_points=reconstructed_data_points,
            created_at=datetime.fromisoformat(ff_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(ff_node["updated_at"].iso_format()),
        )
    return None

async def get_all_financial_forecasts_by_user(session: AsyncSession, user_id: str) -> List[FinancialForecastInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_FORECAST]->(ff:FinancialForecast)
    RETURN ff
    ORDER BY ff.name
    """
    result = await session.run(query, user_id=user_id)
    forecasts = []
    async for record in result:
        ff_node = record["ff"]
        reconstructed_data_points = []
        if ff_node.get("data_points"):
            for dp in ff_node["data_points"]:
                reconstructed_data_points.append(FinancialForecastDataPoint(
                    period=date.fromisoformat(dp["period"]),
                    value_type=dp["value_type"],
                    amount=Decimal(str(dp["amount"]))
                ))
        forecasts.append(FinancialForecastInDB(
            id=ff_node["id"],
            name=ff_node["name"],
            description=ff_node["description"],
            start_date=date.fromisoformat(ff_node["start_date"]),
            end_date=date.fromisoformat(ff_node["end_date"]),
            forecast_type=ff_node["forecast_type"],
            methodology=ff_node["methodology"],
            owner_user_id=ff_node["owner_user_id"],
            data_points=reconstructed_data_points,
            created_at=datetime.fromisoformat(ff_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(ff_node["updated_at"].iso_format()),
        ))
    return forecasts

async def update_financial_forecast(session: AsyncSession, forecast_id: str, forecast_data: FinancialForecastUpdate) -> Optional[FinancialForecastInDB]:
    update_fields = forecast_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_financial_forecast(session, forecast_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "start_date" in update_fields:
        update_fields["start_date"] = update_fields["start_date"].isoformat()
    if "end_date" in update_fields:
        update_fields["end_date"] = update_fields["end_date"].isoformat()
    if "data_points" in update_fields:
        data_points_data = [dp.model_dump() for dp in forecast_data.data_points]
        for dp in data_points_data:
            dp["period"] = dp["period"].isoformat()
            dp["amount"] = float(dp["amount"])
        update_fields["data_points"] = data_points_data

    set_clauses = [f"ff.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (ff:FinancialForecast {{id: $forecast_id}})
    SET {set_query_part}
    RETURN ff
    """
    params = {"forecast_id": forecast_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_financial_forecast(session, forecast_id)
    return None

async def delete_financial_forecast(session: AsyncSession, forecast_id: str) -> bool:
    query = """
    MATCH (ff:FinancialForecast {id: $forecast_id})
    DETACH DELETE ff
    """
    result = await session.run(query, forecast_id=forecast_id)
    return result.consume().counters.nodes_deleted > 0

# --- Scenario CRUD (NEW) ---
async def create_scenario(session: AsyncSession, scenario_data: ScenarioCreate) -> ScenarioInDB:
    scenario_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Convert parameters to dictionaries for storage
    parameters_data = [p.model_dump() for p in scenario_data.parameters]

    query = """
    MATCH (u:User {id: $owner_user_id})
    OPTIONAL MATCH (ff:FinancialForecast {id: $base_forecast_id})
    CREATE (s:Scenario {
        id: $id,
        name: $name,
        description: $description,
        base_forecast_id: $base_forecast_id,
        parameters: $parameters,
        owner_user_id: $owner_user_id,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_SCENARIO]->(s)
    WITH s, ff
    WHERE ff IS NOT NULL
    CREATE (s)-[:BASED_ON_FORECAST]->(ff)
    RETURN s
    """
    params = scenario_data.model_dump()
    params["id"] = scenario_id
    params["parameters"] = parameters_data
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    s_node = record["s"]

    reconstructed_parameters = []
    if s_node.get("parameters"):
        for p in s_node["parameters"]:
            reconstructed_parameters.append(ScenarioParameter(**p))

    return ScenarioInDB(
        id=s_node["id"],
        name=s_node["name"],
        description=s_node["description"],
        base_forecast_id=s_node["base_forecast_id"],
        parameters=reconstructed_parameters,
        owner_user_id=s_node["owner_user_id"],
        created_at=datetime.fromisoformat(s_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(s_node["updated_at"].iso_format()),
    )

async def get_scenario(session: AsyncSession, scenario_id: str) -> Optional[ScenarioInDB]:
    query = """
    MATCH (s:Scenario {id: $scenario_id})
    RETURN s
    """
    result = await session.run(query, scenario_id=scenario_id)
    record = await result.single()

    if record:
        s_node = record["s"]
        reconstructed_parameters = []
        if s_node.get("parameters"):
            for p in s_node["parameters"]:
                reconstructed_parameters.append(ScenarioParameter(**p))
        return ScenarioInDB(
            id=s_node["id"],
            name=s_node["name"],
            description=s_node["description"],
            base_forecast_id=s_node["base_forecast_id"],
            parameters=reconstructed_parameters,
            owner_user_id=s_node["owner_user_id"],
            created_at=datetime.fromisoformat(s_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(s_node["updated_at"].iso_format()),
        )
    return None

async def get_all_scenarios_by_user(session: AsyncSession, user_id: str) -> List[ScenarioInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SCENARIO]->(s:Scenario)
    RETURN s
    ORDER BY s.name
    """
    result = await session.run(query, user_id=user_id)
    scenarios = []
    async for record in result:
        s_node = record["s"]
        reconstructed_parameters = []
        if s_node.get("parameters"):
            for p in s_node["parameters"]:
                reconstructed_parameters.append(ScenarioParameter(**p))
        scenarios.append(ScenarioInDB(
            id=s_node["id"],
            name=s_node["name"],
            description=s_node["description"],
            base_forecast_id=s_node["base_forecast_id"],
            parameters=reconstructed_parameters,
            owner_user_id=s_node["owner_user_id"],
            created_at=datetime.fromisoformat(s_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(s_node["updated_at"].iso_format()),
        ))
    return scenarios

async def update_scenario(session: AsyncSession, scenario_id: str, scenario_data: ScenarioUpdate) -> Optional[ScenarioInDB]:
    update_fields = scenario_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_scenario(session, scenario_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "parameters" in update_fields:
        update_fields["parameters"] = [p.model_dump() for p in scenario_data.parameters]

    set_clauses = [f"s.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (s:Scenario {{id: $scenario_id}})
    SET {set_query_part}
    RETURN s
    """
    params = {"scenario_id": scenario_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_scenario(session, scenario_id)
    return None

async def delete_scenario(session: AsyncSession, scenario_id: str) -> bool:
    query = """
    MATCH (s:Scenario {id: $scenario_id})
    DETACH DELETE s
    """
    result = await session.run(query, scenario_id=scenario_id)
    return result.consume().counters.nodes_deleted > 0
