# ... (existing imports and CRUD operations) ...

# --- Financial Statement Generation (NEW) ---

async def generate_income_statement(session: AsyncSession, start_date: datetime, end_date: datetime) -> IncomeStatement:
    # Aggregates revenues and expenses within a given period
    # Note: Assumes positive for revenues and expenses are handled appropriately in journal entries.
    # In a real system, the 'normal_balance' of the account would dictate if it's a debit or credit
    # to increase the account, and how it impacts IS/BS. For simplicity, we sum debits/credits for IS items.
    query = """
    MATCH (je:JournalEntry)-[:HAS_LINE]->(jl:JournalLine)-[:AFFECTS]->(a:Account)
    WHERE je.entry_date >= datetime($start_date) AND je.entry_date <= datetime($end_date)
    AND a.account_type IN ['Revenue', 'Expense']
    WITH a, 
         SUM(CASE WHEN jl.debit > 0 THEN jl.debit ELSE 0.0 END) AS total_debits,
         SUM(CASE WHEN jl.credit > 0 THEN jl.credit ELSE 0.0 END) AS total_credits
    RETURN 
        a.account_type AS type,
        a.account_name AS category,
        a.normal_balance AS normal_balance,
        total_debits,
        total_credits
    ORDER BY type, category
    """
    result = await session.run(query, start_date=start_date.isoformat(), end_date=end_date.isoformat())

    revenues: List[IncomeStatementItem] = []
    expenses: List[IncomeStatementItem] = []
    total_revenue = Decimal('0.00')
    total_expense = Decimal('0.00')

    async for record in result:
        account_type = record["type"]
        category = record["category"]
        normal_balance = record["normal_balance"]
        total_debits = Decimal(str(record["total_debits"]))
        total_credits = Decimal(str(record["total_credits"]))

        amount = Decimal('0.00')
        # For Income Statement items, usually, revenues increase with credits, expenses with debits
        if normal_balance == 'Credit': # Revenue accounts
            amount = total_credits - total_debits
        elif normal_balance == 'Debit': # Expense accounts
            amount = total_debits - total_credits

        if account_type == 'Revenue':
            revenues.append(IncomeStatementItem(category=category, amount=amount))
            total_revenue += amount
        elif account_type == 'Expense':
            expenses.append(IncomeStatementItem(category=category, amount=amount))
            total_expense += amount
    
    net_income = total_revenue - total_expense

    return IncomeStatement(
        report_date=datetime.utcnow(),
        start_date=start_date,
        end_date=end_date,
        revenues=revenues,
        expenses=expenses,
        net_income=net_income
    )


async def generate_balance_sheet(session: AsyncSession, as_of_date: datetime) -> BalanceSheet:
    # Aggregates balances for Asset, Liability, and Equity accounts up to a specific date.
    # This query calculates the cumulative balance for each account.
    query = """
    MATCH (a:Account)
    OPTIONAL MATCH (jl:JournalLine)-[:AFFECTS]->(a)
    MATCH (je:JournalEntry)-[:HAS_LINE]->(jl)
    WHERE je.entry_date <= datetime($as_of_date)
    AND a.account_type IN ['Asset', 'Liability', 'Equity']
    WITH a, SUM(jl.debit) AS total_debits, SUM(jl.credit) AS total_credits
    RETURN 
        a.account_type AS type,
        a.account_name AS category,
        a.normal_balance AS normal_balance,
        coalesce(total_debits, 0.0) AS sum_debits,
        coalesce(total_credits, 0.0) AS sum_credits
    ORDER BY type, category
    """
    result = await session.run(query, as_of_date=as_of_date.isoformat())

    assets: List[BalanceSheetItem] = []
    liabilities: List[BalanceSheetItem] = []
    equity: List[BalanceSheetItem] = []
    total_assets = Decimal('0.00')
    total_liabilities = Decimal('0.00')
    total_equity = Decimal('0.00')

    async for record in result:
        account_type = record["type"]
        category = record["category"]
        normal_balance = record["normal_balance"]
        sum_debits = Decimal(str(record["sum_debits"]))
        sum_credits = Decimal(str(record["sum_credits"]))

        balance = Decimal('0.00')
        if normal_balance == 'Debit':
            balance = sum_debits - sum_credits
        elif normal_balance == 'Credit':
            balance = sum_credits - sum_debits

        if account_type == 'Asset':
            assets.append(BalanceSheetItem(category=category, amount=balance))
            total_assets += balance
        elif account_type == 'Liability':
            liabilities.append(BalanceSheetItem(category=category, amount=balance))
            total_liabilities += balance
        elif account_type == 'Equity':
            equity.append(BalanceSheetItem(category=category, amount=balance))
            total_equity += balance
    
    # Note: Retained earnings / Net Income for the period would typically flow into Equity
    # but for simplicity at this stage, we're calculating based on balances.
    # A full implementation would involve linking Net Income from the IS to RE on the BS.
    total_liabilities_equity = total_liabilities + total_equity

    return BalanceSheet(
        report_date=datetime.utcnow(),
        as_of_date=as_of_date,
        assets=assets,
        liabilities=liabilities,
        equity=equity,
        total_assets=total_assets,
        total_liabilities_equity=total_liabilities_equity
    )
