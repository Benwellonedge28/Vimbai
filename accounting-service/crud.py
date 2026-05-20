# ... (existing imports and CRUD operations) ...

# Imports for Cash Flow (already included in existing models)
from accounting_service.models import (
    # ... other models ...
    IncomeStatement, BalanceSheet,
    CashFlowStatement, CashFlowSection, CashFlowActivity
)
from datetime import datetime, timedelta # NEW

# ... (existing generate_income_statement and generate_balance_sheet functions) ...

async def generate_cash_flow_statement(
    session: AsyncSession,
    start_date: datetime,
    end_date: datetime,
    net_income: Decimal  # Net income from the Income Statement for the same period
) -> CashFlowStatement:
    # For simplicity, we'll implement an INDIRECT method based on changes in balance sheet accounts
    # This will be a simplified version, a full CFS is very complex without direct cash transaction tagging.

    # Get Balance Sheets at beginning and end of period
    beginning_bs = await generate_balance_sheet(session, start_date - timedelta(microseconds=1)) # As of just before start
    ending_bs = await generate_balance_sheet(session, end_date)

    # Extract cash accounts (assuming account type 'Asset' and name like 'Cash')
    # This is a simplification; a proper cash account identification would be more robust.
    cash_accounts_query = """
        MATCH (a:Account {account_type: 'Asset'})
        WHERE a.account_name CONTAINS 'Cash'
        RETURN a.account_number AS account_number
        """
    cash_account_numbers = [
        r["account_number"] for r in (await session.run(cash_accounts_query)).records()
    ]
    
    beginning_cash_balance_total = Decimal('0.00')
    ending_cash_balance_total = Decimal('0.00')

    for item in beginning_bs.assets:
        # Assuming 'Cash' related assets are the cash balances
        if "Cash" in item.category and item.amount is not None:
            beginning_cash_balance_total += item.amount
    
    for item in ending_bs.assets:
        if "Cash" in item.category and item.amount is not None:
            ending_cash_balance_total += item.amount

    net_increase_decrease_in_cash = ending_cash_balance_total - beginning_cash_balance_total

    # --- Operating Activities (Simplified) ---
    operating_activities_list: List[CashFlowActivity] = []
    net_cash_operating = net_income # Start with net income

    operating_activities_list.append(CashFlowActivity(description="Net Income", amount=net_income))

    # Adjustments for non-cash items and changes in working capital
    # This part is highly simplified and illustrative.
    # A real CFS would compare BS items between periods.
    # Example: Change in Accounts Receivable
    ar_start = Decimal('0.00')
    ar_end = Decimal('0.00')
    for item in beginning_bs.assets:
        if "Accounts Receivable" in item.category:
            ar_start = item.amount
    for item in ending_bs.assets:
        if "Accounts Receivable" in item.category:
            ar_end = item.amount
    change_in_ar = ar_end - ar_start
    operating_activities_list.append(CashFlowActivity(description="Decrease (Increase) in Accounts Receivable", amount=-change_in_ar))
    net_cash_operating -= change_in_ar # Increase in AR decreases cash

    # Example: Change in Accounts Payable
    ap_start = Decimal('0.00')
    ap_end = Decimal('0.00')
    for item in beginning_bs.liabilities:
        if "Accounts Payable" in item.category:
            ap_start = item.amount
    for item in ending_bs.liabilities:
        if "Accounts Payable" in item.category:
            ap_end = item.amount
    change_in_ap = ap_end - ap_start
    operating_activities_list.append(CashFlowActivity(description="Increase (Decrease) in Accounts Payable", amount=change_in_ap))
    net_cash_operating += change_in_ap # Increase in AP increases cash


    operating_activities = CashFlowSection(
        title="Cash Flow from Operating Activities",
        activities=operating_activities_list,
        net_cash=net_cash_operating
    )

    # --- Investing Activities (Simplified) ---
    investing_activities_list: List[CashFlowActivity] = []
    net_cash_investing = Decimal('0.00')
    # Example: Purchase/Sale of Equipment (change in Fixed Assets)
    fixed_assets_start = Decimal('0.00')
    fixed_assets_end = Decimal('0.00')
    for item in beginning_bs.assets:
        if "Equipment" in item.category or "Property, Plant, & Equipment" in item.category:
            fixed_assets_start += item.amount
    for item in ending_bs.assets:
        if "Equipment" in item.category or "Property, Plant, & Equipment" in item.category:
            fixed_assets_end += item.amount
    change_in_fixed_assets = fixed_assets_end - fixed_assets_start
    investing_activities_list.append(CashFlowActivity(description="Purchase (Sale) of Fixed Assets", amount=-change_in_fixed_assets))
    net_cash_investing -= change_in_fixed_assets # Increase in Fixed Assets decreases cash

    investing_activities = CashFlowSection(
        title="Cash Flow from Investing Activities",
        activities=investing_activities_list,
        net_cash=net_cash_investing
    )

    # --- Financing Activities (Simplified) ---
    financing_activities_list: List[CashFlowActivity] = []
    net_cash_financing = Decimal('0.00')
    # Example: Issuance/Repayment of Debt (change in Notes Payable)
    notes_payable_start = Decimal('0.00')
    notes_payable_end = Decimal('0.00')
    for item in beginning_bs.liabilities:
        if "Notes Payable" in item.category:
            notes_payable_start = item.amount
    for item in ending_bs.liabilities:
        if "Notes Payable" in item.category:
            notes_payable_end = item.amount
    change_in_notes_payable = notes_payable_end - notes_payable_start
    financing_activities_list.append(CashFlowActivity(description="Issuance (Repayment) of Debt", amount=change_in_notes_payable))
    net_cash_financing += change_in_notes_payable # Increase in Notes Payable increases cash

    financing_activities = CashFlowSection(
        title="Cash Flow from Financing Activities",
        activities=financing_activities_list,
        net_cash=net_cash_financing
    )

    # Verify net increase/decrease
    calculated_net_cash_flow = net_cash_operating + net_cash_investing + net_cash_financing
    # In a real system, you'd compare calculated_net_cash_flow with (ending_cash - beginning_cash)

    return CashFlowStatement(
        report_date=datetime.utcnow(),
        start_date=start_date,
        end_date=end_date,
        net_income=net_income,
        operating_activities=operating_activities,
        investing_activities=investing_activities,
        financing_activities=financing_activities,
        net_increase_decrease_in_cash=calculated_net_cash_flow,
        beginning_cash_balance=beginning_cash_balance_total,
        ending_cash_balance=ending_cash_balance_total,
    )
