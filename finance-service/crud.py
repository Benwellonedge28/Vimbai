# ... (existing imports and CRUD operations) ...

from finance_service.models import (
    BudgetCreate, BudgetUpdate, BudgetInDB, BudgetItemCreate, BudgetItemInDB,
    ActualsSummary, BudgetVarianceItem, BudgetVarianceReport,
    LiquidityRatios, SolvencyRatios, ProfitabilityRatios, FinancialRatiosReport # NEW
)
from datetime import datetime
import uuid
from decimal import Decimal
import httpx # NEW
import os

# Internal API Gateway URL for service-to-service communication
API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")


async def fetch_income_statement_from_accounting(jwt_token: str, start_date: datetime, end_date: datetime) -> Optional[dict]:
    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    params = {
        "start_date": start_date.isoformat(),
        "end_date": end_date.isoformat(),
    }
    async with httpx.AsyncClient() as client:
        response = await client.get(
            f"{API_GATEWAY_URL}/financial-statements/income-statement",
            headers=headers,
            params=params
        )
    response.raise_for_status() # Raise an exception for HTTP errors
    return response.json()

async def fetch_balance_sheet_from_accounting(jwt_token: str, as_of_date: datetime) -> Optional[dict]:
    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    params = {
        "as_of_date": as_of_date.isoformat(),
    }
    async with httpx.AsyncClient() as client:
        response = await client.get(
            f"{API_GATEWAY_URL}/financial-statements/balance-sheet",
            headers=headers,
            params=params
        )
    response.raise_for_status() # Raise an exception for HTTP errors
    return response.json()

async def generate_financial_ratios_report(jwt_token: str, start_date: datetime, end_date: datetime) -> FinancialRatiosReport:
    # Fetch Income Statement and Balance Sheet from Accounting Service via API Gateway
    is_data = await fetch_income_statement_from_accounting(jwt_token, start_date, end_date)
    bs_data = await fetch_balance_sheet_from_accounting(jwt_token, end_date) # BS is as of end_date

    if not is_data or not bs_data:
        raise ValueError("Could not retrieve required financial statements from Accounting Service.")

    # Extract relevant figures (using Decimal for precision)
    # Income Statement data
    total_revenue = Decimal('0.00')
    for r in is_data.get('revenues', []):
        total_revenue += Decimal(str(r['amount']))
    
    cost_of_goods_sold = Decimal('0.00') # Placeholder, would need to identify COGS accounts
    # For simplicity, calculate Gross Profit as Net Income + Total Expenses
    net_income = Decimal(str(is_data.get('net_income', '0.00')))
    total_expenses = Decimal('0.00')
    for e in is_data.get('expenses', []):
        total_expenses += Decimal(str(e['amount']))
    
    gross_profit = total_revenue - cost_of_goods_sold # This needs proper COGS
    # For this POC, let's use net_income as a base for profitability until COGS is defined.

    # Balance Sheet data
    current_assets = Decimal('0.00')
    total_assets = Decimal('0.00')
    for a in bs_data.get('assets', []):
        if "Cash" in a['category'] or "Accounts Receivable" in a['category'] or "Inventory" in a['category']: # Simplified current asset check
            current_assets += Decimal(str(a['amount']))
        total_assets += Decimal(str(a['amount']))
    
    current_liabilities = Decimal('0.00')
    total_liabilities = Decimal('0.00') # Summing all liabilities for total_debt
    for l in bs_data.get('liabilities', []):
        if "Accounts Payable" in l['category'] or "Short-term Loans" in l['category']: # Simplified current liability check
            current_liabilities += Decimal(str(l['amount']))
        total_liabilities += Decimal(str(l['amount']))
    
    total_equity = Decimal('0.00')
    for e in bs_data.get('equity', []):
        total_equity += Decimal(str(e['amount']))
    
    # Calculate Ratios
    liquidity = LiquidityRatios()
    if current_liabilities != Decimal('0.00'):
        liquidity.current_ratio = current_assets / current_liabilities
        # quick_ratio calculation would need to exclude inventory from current assets

    solvency = SolvencyRatios()
    if total_equity != Decimal('0.00'):
        solvency.debt_to_equity_ratio = total_liabilities / total_equity
    if total_assets != Decimal('0.00'):
        solvency.debt_to_asset_ratio = total_liabilities / total_assets

    profitability = ProfitabilityRatios()
    if total_revenue != Decimal('0.00'):
        # profitability.gross_profit_margin = (total_revenue - cost_of_goods_sold) / total_revenue # Needs COGS
        profitability.net_profit_margin = net_income / total_revenue
    if total_assets != Decimal('0.00'):
        profitability.return_on_assets = net_income / total_assets


    return FinancialRatiosReport(
        report_date=datetime.utcnow(),
        start_date=start_date,
        end_date=end_date,
        liquidity=liquidity,
        solvency=solvency,
        profitability=profitability
    )
