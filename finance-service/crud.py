from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from finance_service.models import (
    BudgetCreate, BudgetUpdate, BudgetInDB, BudgetItemCreate, BudgetItemInDB,
    ActualsSummary, BudgetVarianceItem, BudgetVarianceReport,
    LiquidityRatios, SolvencyRatios, ProfitabilityRatios,
    EfficiencyRatios, MarketValueRatios, FinancialRatiosReport 
)
from datetime import datetime
import uuid
from decimal import Decimal
import httpx
import os
from finance_service.exceptions import ValidationError # NEW

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# ... (existing fetch_income_statement_from_accounting, fetch_balance_sheet_from_accounting functions) ...

# Helper to extract a value from a statement section by category name
def _get_amount_by_category(items_list: List[Dict], category_name: str) -> Decimal:
    for item in items_list:
        if item.get('category') == category_name:
            return Decimal(str(item.get('amount', '0.00')))
    return Decimal('0.00')

async def generate_financial_ratios_report(jwt_token: str, start_date: datetime, end_date: datetime) -> FinancialRatiosReport:
    is_data = await fetch_income_statement_from_accounting(jwt_token, start_date, end_date)
    bs_data_end = await fetch_balance_sheet_from_accounting(jwt_token, end_date) # BS at end of period

    # For ratios requiring averages (e.g., Average Inventory, Average Assets), we need a beginning balance sheet
    # For this implementation, we'll simplify and assume end-of-period values or average them conceptually.
    # A true "average" would need BS data from start_date and end_date.
    bs_data_start = await fetch_balance_sheet_from_accounting(jwt_token, start_date) # NEW: For averages

    if not is_data or not bs_data_end or not bs_data_start:
        raise ValidationError(detail="Could not retrieve required financial statements from Accounting Service.", code="FINANCIAL_STATEMENTS_UNAVAILABLE")

    # --- Extract IS Data ---
    total_revenue = Decimal('0.00') 
    if is_data.get('revenues'):
        total_revenue = sum(Decimal(str(item['amount'])) for item in is_data['revenues'])
    
    # Cost of Goods Sold - assuming it's an expense category. Needs explicit mapping in a real system.
    cost_of_goods_sold = _get_amount_by_category(is_data.get('expenses', []), "Cost of Goods Sold")
    
    # Operating Expenses - needs more granular IS data to accurately separate COGS from other operating expenses.
    # For now, let's assume all expenses in 'expenses' list, except interest, are operating expenses for EBIT calculation. 
    all_expenses = sum(Decimal(str(item['amount'])) for item in is_data['expenses'])
    interest_expense = _get_amount_by_category(is_data.get('expenses', []), "Interest Expense")
    operating_expenses = all_expenses - interest_expense # Simplified for now

    net_income = Decimal(str(is_data.get('net_income', '0.00')))

    # Calculate Gross Profit (Revenue - COGS)
    gross_profit = total_revenue - cost_of_goods_sold
    
    # Calculate Operating Income (EBIT) (Gross Profit - Operating Expenses)
    operating_income = gross_profit - operating_expenses

    # --- Extract BS Data (End of Period) ---
    # Assets
    current_assets = sum(Decimal(str(a['amount'])) for a in bs_data_end.get('assets', []) if a['category'] in ["Cash", "Accounts Receivable", "Inventory", "Prepaid Expenses"])
    cash = _get_amount_by_category(bs_data_end.get('assets', []), "Cash")
    marketable_securities = _get_amount_by_category(bs_data_end.get('assets', []), "Marketable Securities") # Needs specific COA mapping
    accounts_receivable = _get_amount_by_category(bs_data_end.get('assets', []), "Accounts Receivable")
    inventory = _get_amount_by_category(bs_data_end.get('assets', []), "Inventory")
    total_assets_end = sum(Decimal(str(a['amount'])) for a in bs_data_end.get('assets', []))
    
    # Liabilities
    current_liabilities = sum(Decimal(str(l['amount'])) for l in bs_data_end.get('liabilities', []) if l['category'] in ["Accounts Payable", "Short-term Loans", "Accrued Expenses", "Current Portion of Long-term Debt"])
    accounts_payable = _get_amount_by_category(bs_data_end.get('liabilities', []), "Accounts Payable")
    total_liabilities_end = sum(Decimal(str(l['amount'])) for l in bs_data_end.get('liabilities', [])) # Total debt
    
    # Equity
    total_equity_end = sum(Decimal(str(e['amount'])) for e in bs_data_end.get('equity', []))
    
    # --- Extract BS Data (Start of Period for Averages) ---
    total_assets_start = sum(Decimal(str(a['amount'])) for a in bs_data_start.get('assets', []))
    accounts_receivable_start = _get_amount_by_category(bs_data_start.get('assets', []), "Accounts Receivable")
    inventory_start = _get_amount_by_category(bs_data_start.get('assets', []), "Inventory")
    accounts_payable_start = _get_amount_by_category(bs_data_start.get('liabilities', []), "Accounts Payable")

    # --- Calculate Averages (Simplified: (Start + End) / 2) ---
    avg_total_assets = (total_assets_start + total_assets_end) / Decimal('2.00') if (total_assets_start + total_assets_end) != 0 else Decimal('0.00')
    avg_accounts_receivable = (accounts_receivable_start + accounts_receivable) / Decimal('2.00') if (accounts_receivable_start + accounts_receivable) != 0 else Decimal('0.00')
    avg_inventory = (inventory_start + inventory) / Decimal('2.00') if (inventory_start + inventory) != 0 else Decimal('0.00')
    avg_accounts_payable = (accounts_payable_start + accounts_payable) / Decimal('2.00') if (accounts_payable_start + accounts_payable) != 0 else Decimal('0.00')

    # --- Calculate Ratios ---
    liquidity = LiquidityRatios()
    if current_liabilities > 0:
        liquidity.current_ratio = current_assets / current_liabilities
        # Quick Ratio (Cash + Marketable Securities + Accounts Receivable) / Current Liabilities
        # Assuming marketable securities are part of current_assets, otherwise adjust
        liquidity.quick_ratio = (current_assets - inventory) / current_liabilities # Simplified assumption
        liquidity.cash_ratio = cash / current_liabilities
    liquidity.working_capital = current_assets - current_liabilities

    solvency = SolvencyRatios()
    if total_equity_end > 0:
        solvency.debt_to_equity_ratio = total_liabilities_end / total_equity_end
    if total_assets_end > 0:
        solvency.debt_to_asset_ratio = total_liabilities_end / total_assets_end
    if total_equity_end > 0:
        solvency.equity_multiplier = total_assets_end / total_equity_end
    if interest_expense > 0: 
        solvency.times_interest_earned = operating_income / interest_expense 

    profitability = ProfitabilityRatios()
    if total_revenue > 0:
        profitability.gross_profit_margin = gross_profit / total_revenue
        profitability.operating_profit_margin = operating_income / total_revenue
        profitability.net_profit_margin = net_income / total_revenue
    if total_assets_end > 0: # Using end of period assets for ROA
        profitability.return_on_assets = net_income / total_assets_end
    if total_equity_end > 0:
        profitability.return_on_equity = net_income / total_equity_end
    
    efficiency = EfficiencyRatios()
    if avg_inventory > 0:
        efficiency.inventory_turnover = cost_of_goods_sold / avg_inventory 
    if avg_accounts_receivable > 0: # Using total_revenue as sales
        efficiency.accounts_receivable_turnover = total_revenue / avg_accounts_receivable 
        if efficiency.accounts_receivable_turnover > 0: # Avoid division by zero
            efficiency.day_sales_outstanding = Decimal('365.00') / efficiency.accounts_receivable_turnover
    if avg_accounts_payable > 0: # Needs COGS
        efficiency.accounts_payable_turnover = cost_of_goods_sold / avg_accounts_payable 
    if avg_total_assets > 0: # Using total_revenue as sales
        efficiency.asset_turnover = total_revenue / avg_total_assets 

    market_value = MarketValueRatios() # These ratios typically require public company data

    return FinancialRatiosReport(
        report_date=datetime.utcnow(),
        start_date=start_date,
        end_date=end_date,
        liquidity=liquidity,
        solvency=solvency,
        profitability=profitability,
        efficiency=efficiency,
        market_value=market_value,
    )
