# ... (existing imports and CRUD operations) ...

async def get_actuals_for_period(
    session: AsyncSession, # Using Finance Service's Neo4j session for now, ideally this would be an API call to Accounting Service
    account_numbers: List[str],
    start_date: datetime,
    end_date: datetime
) -> Dict[str, ActualsSummary]:
    # In a real microservice architecture, this would be an HTTP call to the Accounting Service
    # For this implementation, we will simulate this by directly querying the Accounting Service's Neo4j data
    # (assuming Finance Service has access to Accounting Service's data or there's a shared DB for simplicity of POC)
    # Or more realistically, we would perform an HTTP GET request to Accounting Service's ledger endpoint.

    # Simulate fetching actuals from Accounting Service (direct Neo4j access for POC)
    query = """
    MATCH (a:Account)
    WHERE a.account_number IN $account_numbers
    OPTIONAL MATCH (jl:JournalLine)-[:AFFECTS]->(a)
    MATCH (je:JournalEntry)-[:HAS_LINE]->(jl)
    WHERE je.entry_date >= datetime($start_date) AND je.entry_date <= datetime($end_date)
    RETURN
        a.account_number AS account_number,
        a.normal_balance AS normal_balance,
        coalesce(SUM(jl.debit), 0.0) AS total_debit,
        coalesce(SUM(jl.credit), 0.0) AS total_credit
    """
    result = await session.run(query, account_numbers=account_numbers, start_date=start_date.isoformat(), end_date=end_date.isoformat())

    actuals_summary: Dict[str, ActualsSummary] = {}
    async for record in result:
        account_number = record["account_number"]
        normal_balance = record["normal_balance"]
        total_debit = Decimal(str(record["total_debit"]))
        total_credit = Decimal(str(record["total_credit"]))

        balance = Decimal('0.00')
        if normal_balance == 'Debit':
            balance = total_debit - total_credit
        elif normal_balance == 'Credit':
            balance = total_credit - total_debit
        
        actuals_summary[account_number] = ActualsSummary(
            account_number=account_number,
            total_debit=total_debit,
            total_credit=total_credit,
            balance=balance
        )
    return actuals_summary


async def generate_budget_variance_report(session: AsyncSession, budget_id: str) -> Optional[BudgetVarianceReport]:
    budget = await get_budget(session, budget_id)
    if not budget:
        return None

    # Collect all unique account numbers and the earliest/latest dates from budget items
    account_numbers_in_budget = [item.account_number for item in budget.items if item.account_number]
    min_date = min(item.period_start for item in budget.items)
    max_date = max(item.period_end for item in budget.items)

    actuals_summary = await get_actuals_for_period(
        session,
        account_numbers_in_budget,
        min_date,
        max_date
    )

    variance_items: List[BudgetVarianceItem] = []
    total_budgeted = Decimal('0.00')
    total_actual = Decimal('0.00')

    for item in budget.items:
        actual_amount = Decimal('0.00')
        if item.account_number in actuals_summary:
            actual_amount = actuals_summary[item.account_number].balance # Use the balance from the actuals

        variance = actual_amount - item.budgeted_amount
        variance_percentage = (float(variance) / float(item.budgeted_amount) * 100) if item.budgeted_amount != 0 else 0.0

        variance_items.append(BudgetVarianceItem(
            category=item.category,
            account_number=item.account_number,
            budgeted_amount=item.budgeted_amount,
            actual_amount=actual_amount,
            variance=variance,
            variance_percentage=variance_percentage
        ))
        total_budgeted += item.budgeted_amount
        total_actual += actual_amount
    
    total_variance = total_actual - total_budgeted
    total_variance_percentage = (float(total_variance) / float(total_budgeted) * 100) if total_budgeted != 0 else 0.0

    return BudgetVarianceReport(
        budget_name=budget.name,
        fiscal_year=budget.fiscal_year,
        period=budget.period,
        items=variance_items,
        total_budgeted=total_budgeted,
        total_actual=total_actual,
        total_variance=total_variance,
        total_variance_percentage=total_variance_percentage
    )
