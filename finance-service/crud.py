from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from finance_service.models import (
    BudgetCreate, BudgetUpdate, BudgetInDB, BudgetItemCreate, BudgetItemInDB,
    ActualsSummary, BudgetVarianceItem, BudgetVarianceReport,
    LiquidityRatios, SolvencyRatios, ProfitabilityRatios, FinancialRatiosReport, # Existing
    EfficiencyRatios, MarketRatios # NEW
)
from datetime import datetime
import uuid
from decimal import Decimal
import httpx
import os
from finance_service.exceptions import ValidationError, NotFoundError # NEW import for consistency

# Internal API Gateway URL for service-to-service communication
API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# ... (existing Budget CRUD and get_actuals_for_period) ...

# --- Accounting Service Data Fetching ---
async def fetch_income_statement_from_accounting(jwt_token: str, start_date: datetime, end_date: datetime) -> Dict[str, Any]:
    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    params = {
        "start_date": start_date.isoformat(),
        "end_date": end_date.isoformat(),
    }
    async with httpx.AsyncClient() as client:
        try:
            response = await client.get(
                f"{API_GATEWAY_URL}/financial-statements/income-statement",
                headers=headers,
                params=params
            )
            response.raise_for_status() # Raise an exception for HTTP errors (4xx or 5xx)
            return response.json()
        except httpx.HTTPStatusError as e:
            # Re-raise with a more specific error for upstream failure
            raise ValidationError(detail=f"Failed to fetch Income Statement: {e.response.status_code} - {e.response.text}", code="UPSTREAM_IS_FETCH_FAILED")
        except httpx.RequestError as e:
            raise ValidationError(detail=f"Network error fetching Income Statement: {e}", code="UPSTREAM_IS_NETWORK_ERROR")

async def fetch_balance_sheet_from_accounting(jwt_token: str, as_of_date: datetime) -> Dict[str, Any]:
    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    params = {
        "as_of_date": as_of_date.isoformat(),
    }
    async with httpx.AsyncClient() as client:
        try:
            response = await client.get(
                f"{API_GATEWAY_URL}/financial-statements/balance-sheet",
                headers=headers,
                params=params
            )
            response.raise_for_status() # Raise an exception for HTTP errors (4xx or 5xx)
            return response.json()
        except httpx.HTTPStatusError as e:
            raise ValidationError(detail=f"Failed to fetch Balance Sheet: {e.response.status_code} - {e.response.text}", code="UPSTREAM_BS_FETCH_FAILED")
        except httpx.RequestError as e:
            raise ValidationError(detail=f"Network error fetching Balance Sheet: {e}", code="UPSTREAM_BS_NETWORK_ERROR")

# --- Financial Ratio Calculations (EXPANDED) ---
async def generate_financial_ratios_report(jwt_token: str, start_date: datetime, end_date: datetime) -> FinancialRatiosReport:
    is_data = await fetch_income_statement_from_accounting(jwt_token, start_date, end_date)
    bs_data = await fetch_balance_sheet_from_accounting(jwt_token, end_date) # BS is as of end_date

    # Extract relevant figures with Decimal for precision, default to 0 if not found
    # Income Statement Data
    total_revenue = Decimal(str(next((item['amount'] for item in is_data.get('revenues', []) if item['category'] == 'Total Revenue'), '0.00')))
    cost_of_goods_sold = Decimal(str(next((item['amount'] for item in is_data.get('expenses', []) if item['category'] == 'Cost of Goods Sold'), '0.00')))
    gross_profit = total_revenue - cost_of_goods_sold # Assuming COGS is explicitly available from IS
    operating_income = Decimal(str(next((item['amount'] for item in is_data.get('expenses', []) if item['category'] == 'Operating Income'), '0.00'))) # Placeholder, would need detailed IS
    net_income = Decimal(str(is_data.get('net_income', '0.00')))
    interest_expense = Decimal(str(next((item['amount'] for item in is_data.get('expenses', []) if item['category'] == 'Interest Expense'), '0.00')))
    tax_expense = Decimal(str(next((item['amount'] for item in is_data.get('expenses', []) if item['category'] == 'Tax Expense'), '0.00')))
    # Shares outstanding (external) = Decimal('1000000') # For EPS, requires external input

    # Balance Sheet Data
    cash = Decimal(str(next((item['amount'] for item in bs_data.get('assets', []) if item['category'] == 'Cash'), '0.00')))
    marketable_securities = Decimal(str(next((item['amount'] for item in bs_data.get('assets', []) if item['category'] == 'Marketable Securities'), '0.00')))
    accounts_receivable = Decimal(str(next((item['amount'] for item in bs_data.get('assets', []) if item['category'] == 'Accounts Receivable'), '0.00')))
    inventory = Decimal(str(next((item['amount'] for item in bs_data.get('assets', []) if item['category'] == 'Inventory'), '0.00')))
    total_current_assets = Decimal(str(next((item['amount'] for item in bs_data.get('assets', []) if item['category'] == 'Total Current Assets'), '0.00')))
    total_assets = Decimal(str(bs_data.get('total_assets', '0.00')))
    
    accounts_payable = Decimal(str(next((item['amount'] for item in bs_data.get('liabilities', []) if item['category'] == 'Accounts Payable'), '0.00')))
    total_current_liabilities = Decimal(str(next((item['amount'] for item in bs_data.get('liabilities', []) if item['category'] == 'Total Current Liabilities'), '0.00')))
    total_liabilities = Decimal(str(bs_data.get('total_liabilities', '0.00'))) # Assuming total_liabilities from BS
    total_equity = Decimal(str(bs_data.get('total_liabilities_equity', '0.00'))) # In our BS, this is total L&E, so total equity

    # Initialize all ratio categories
    liquidity = LiquidityRatios()
    solvency = SolvencyRatios()
    profitability = ProfitabilityRatios()
    efficiency = EfficiencyRatios()
    market = MarketRatios()

    # Calculate Liquidity Ratios
    if total_current_liabilities > 0:
        liquidity.current_ratio = total_current_assets / total_current_liabilities
        liquidity.quick_ratio = (cash + marketable_securities + accounts_receivable) / total_current_liabilities
        liquidity.cash_ratio = (cash + marketable_securities) / total_current_liabilities
    
    # Calculate Solvency Ratios
    if total_equity > 0: # Avoid division by zero
        solvency.debt_to_equity_ratio = total_liabilities / total_equity
    if total_assets > 0: # Avoid division by zero
        solvency.debt_to_asset_ratio = total_liabilities / total_assets
        if total_equity > 0: # Avoid division by zero
            solvency.equity_multiplier = total_assets / total_equity
    # Interest Coverage Ratio
    ebit = net_income + interest_expense + tax_expense # Simplified assumption for EBIT
    if interest_expense > 0: # Avoid division by zero
        solvency.interest_coverage_ratio = ebit / interest_expense

    # Calculate Profitability Ratios
    if total_revenue > 0: # Avoid division by zero
        if cost_of_goods_sold > 0: # Only if COGS is meaningful
            profitability.gross_profit_margin = (total_revenue - cost_of_goods_sold) / total_revenue
        # profitability.operating_profit_margin = operating_income / total_revenue # Requires explicit Operating Income
        profitability.net_profit_margin = net_income / total_revenue
    if total_assets > 0: # Avoid division by zero
        profitability.return_on_assets = net_income / total_assets
    if total_equity > 0: # Avoid division by zero
        profitability.return_on_equity = net_income / total_equity
    
    capital_employed = total_assets - total_current_liabilities # Simplified Capital Employed
    if capital_employed > 0 and ebit > 0: # Avoid division by zero
        profitability.return_on_capital_employed = ebit / capital_employed
    
    # profitability.earnings_per_share = net_income / shares_outstanding # Requires shares outstanding

    # Calculate Efficiency Ratios
    if cost_of_goods_sold > 0 and inventory > 0: # Assuming current inventory is average
        efficiency.inventory_turnover = cost_of_goods_sold / inventory
    if total_revenue > 0 and accounts_receivable > 0: # Assuming credit sales = total revenue
        efficiency.accounts_receivable_turnover = total_revenue / accounts_receivable
        if efficiency.accounts_receivable_turnover and efficiency.accounts_receivable_turnover > 0: # Avoid division by zero
            efficiency.days_sales_outstanding = Decimal(365) / efficiency.accounts_receivable_turnover
    if cost_of_goods_sold > 0 and accounts_payable > 0: # Assuming current accounts payable is average
        efficiency.accounts_payable_turnover = cost_of_goods_sold / accounts_payable
    if total_revenue > 0 and total_assets > 0: # Assuming average assets = current assets
        efficiency.asset_turnover = total_revenue / total_assets
    if efficiency.inventory_turnover and efficiency.inventory_turnover > 0: # Avoid division by zero
        efficiency.days_inventory_outstanding = Decimal(365) / efficiency.inventory_turnover

    # Market Ratios (placeholders, require external data)
    # market.price_earnings_ratio = share_price / profitability.earnings_per_share
    # market.dividend_yield = annual_dividend_per_share / share_price

    return FinancialRatiosReport(
        report_date=datetime.utcnow(),
        start_date=start_date,
        end_date=end_date,
        liquidity=liquidity,
        solvency=solvency,
        profitability=profitability,
        efficiency=efficiency,
        market=market,
    )
