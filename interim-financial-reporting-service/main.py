"""
Interim Financial Reporting Service
Port: 8141
Generates quarterly/half-year financial statements with comparative figures
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Interim Financial Reporting Service", version="1.0.0")

# Pydantic Models
class InterimPeriod(BaseModel):
    period_type: str = Field(pattern="^(quarterly|half_year|nine_months)$")
    year: int
    period_number: int = Field(ge=1, le=4)
    start_date: str
    end_date: str

class InterimRevenue(BaseModel):
    revenue: float
    cost_of_sales: float
    gross_profit: float
    other_income: float
    distribution_costs: float
    administrative_expenses: float
    operating_profit: float
    finance_costs: float
    profit_before_tax: float
    income_tax_expense: float
    profit_after_tax: float

class InterimBalanceSheet(BaseModel):
    assets: Dict[str, float]
    liabilities: Dict[str, float]
    equity: Dict[str, float]
    total_assets: float
    total_liabilities: float
    total_equity: float

class InterimStatementOfChangesInEquity(BaseModel):
    opening_balance: float
    profit_for_period: float
    other_comprehensive_income: float
    dividends_paid: float
    share_issue: float
    closing_balance: float

class InterimCashFlow(BaseModel):
    operating_activities: float
    investing_activities: float
    financing_activities: float
    net_cash_flow: float
    opening_cash: float
    closing_cash: float

class EarningsPerShare(BaseModel):
    basic_eps: float
    diluted_eps: float
    weighted_average_shares: int
    diluted_shares: int

class InterimReportRequest(BaseModel):
    company_id: str
    period: InterimPeriod
    include_comparatives: bool = True
    include_segment_breakdown: bool = False
    accounting_standard: str = "IFRS"  # IFRS or US GAAP

class InterimReportResponse(BaseModel):
    company_id: str
    period: InterimPeriod
    currency: str = "USD"
    revenue: InterimRevenue
    comparative_revenue: Optional[InterimRevenue] = None
    balance_sheet: InterimBalanceSheet
    comparative_balance_sheet: Optional[InterimBalanceSheet] = None
    changes_in_equity: InterimStatementOfChangesInEquity
    cash_flow: InterimCashFlow
    earnings_per_share: EarningsPerShare
    year_to_date: InterimRevenue
    effective_tax_rate: float
    management_commentary: Dict[str, str]

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal Vimbai service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "interim-financial-reporting", "version": "1.0.0"}

@app.post("/prepare", response_model=InterimReportResponse)
async def prepare_interim_report(request: InterimReportRequest):
    """Prepare comprehensive interim financial report."""
    logger.info("Preparing interim report", company=request.company_id, period=request.period.end_date)

    multiplier = request.period.period_number

    # Current period figures
    revenue = 2500000.0 * multiplier
    cost_of_sales = revenue * 0.55
    gross_profit = revenue - cost_of_sales
    other_income = revenue * 0.03
    distribution_costs = revenue * 0.08
    admin_expenses = revenue * 0.12
    operating_profit = gross_profit + other_income - distribution_costs - admin_expenses
    finance_costs = operating_profit * 0.05
    profit_before_tax = operating_profit - finance_costs
    income_tax = profit_before_tax * 0.25
    profit_after_tax = profit_before_tax - income_tax

    current_revenue = InterimRevenue(
        revenue=revenue,
        cost_of_sales=cost_of_sales,
        gross_profit=gross_profit,
        other_income=other_income,
        distribution_costs=distribution_costs,
        administrative_expenses=admin_expenses,
        operating_profit=operating_profit,
        finance_costs=finance_costs,
        profit_before_tax=profit_before_tax,
        income_tax_expense=income_tax,
        profit_after_tax=profit_after_tax
    )

    # Year-to-date figures
    ytd_revenue = revenue
    ytd_cost = cost_of_sales
    ytd_gross = gross_profit
    ytd_other_income = other_income
    ytd_dist = distribution_costs
    ytd_admin = admin_expenses
    ytd_operating = operating_profit
    ytd_finance = finance_costs
    ytd_pbt = profit_before_tax
    ytd_tax = income_tax
    ytd_pat = profit_after_tax

    ytd_revenue_data = InterimRevenue(
        revenue=ytd_revenue,
        cost_of_sales=ytd_cost,
        gross_profit=ytd_gross,
        other_income=ytd_other_income,
        distribution_costs=ytd_dist,
        administrative_expenses=ytd_admin,
        operating_profit=ytd_operating,
        finance_costs=ytd_finance,
        profit_before_tax=ytd_pbt,
        income_tax_expense=ytd_tax,
        profit_after_tax=ytd_pat
    )

    # Comparative figures (prior year same period)
    comparative_revenue = None
    if request.include_comparatives:
        comp_mult = request.period.period_number
        comp_revenue = 2300000.0 * comp_mult
        comp_cos = comp_revenue * 0.58
        comp_gp = comp_revenue - comp_cos
        comp_oi = comp_revenue * 0.025
        comp_dist = comp_revenue * 0.07
        comp_admin = comp_revenue * 0.11
        comp_op = comp_gp + comp_oi - comp_dist - comp_admin
        comp_fin = comp_op * 0.06
        comp_pbt = comp_op - comp_fin
        comp_tax = comp_pbt * 0.25
        comp_pat = comp_pbt - comp_tax

        comparative_revenue = InterimRevenue(
            revenue=comp_revenue,
            cost_of_sales=comp_cos,
            gross_profit=comp_gp,
            other_income=comp_oi,
            distribution_costs=comp_dist,
            administrative_expenses=comp_admin,
            operating_profit=comp_op,
            finance_costs=comp_fin,
            profit_before_tax=comp_pbt,
            income_tax_expense=comp_tax,
            profit_after_tax=comp_pat
        )

    # Balance sheet
    total_assets = revenue * 8
    total_liabilities = total_assets * 0.6
    total_equity = total_assets - total_liabilities

    balance_sheet = InterimBalanceSheet(
        assets={
            "property_plant_equipment": total_assets * 0.35,
            "intangible_assets": total_assets * 0.15,
            "inventory": total_assets * 0.12,
            "trade_receivables": total_assets * 0.15,
            "cash_and_equivalents": total_assets * 0.10,
            "other_current_assets": total_assets * 0.13
        },
        liabilities={
            "long_term_borrowings": total_liabilities * 0.30,
            "deferred_tax": total_liabilities * 0.10,
            "trade_payables": total_liabilities * 0.20,
            "short_term_borrowings": total_liabilities * 0.15,
            "current_tax": total_liabilities * 0.08,
            "accruals": total_liabilities * 0.17
        },
        equity={
            "share_capital": total_equity * 0.20,
            "share_premium": total_equity * 0.15,
            "retained_earnings": total_equity * 0.50,
            "other_reserves": total_equity * 0.15
        },
        total_assets=total_assets,
        total_liabilities=total_liabilities,
        total_equity=total_equity
    )

    # Changes in equity
    changes_in_equity = InterimStatementOfChangesInEquity(
        opening_balance=total_equity * 0.85,
        profit_for_period=profit_after_tax,
        other_comprehensive_income=50000.0,
        dividends_paid=-150000.0,
        share_issue=0.0,
        closing_balance=total_equity
    )

    # Cash flow
    cash_flow = InterimCashFlow(
        operating_activities=profit_after_tax + depreciation_calc(total_assets),
        investing_activities=-300000.0 * multiplier,
        financing_activities=-100000.0 * multiplier,
        net_cash_flow=200000.0,
        opening_cash=total_assets * 0.08,
        closing_cash=total_assets * 0.10
    )

    # EPS
    weighted_avg_shares = 10000000
    diluted_shares = 10500000
    basic_eps = profit_after_tax / weighted_avg_shares
    diluted_eps = profit_after_tax / diluted_shares

    earnings_per_share = EarningsPerShare(
        basic_eps=basic_eps,
        diluted_eps=diluted_eps,
        weighted_average_shares=weighted_avg_shares,
        diluted_shares=diluted_shares
    )

    # Management commentary
    commentary = {
        "revenue_change": f"+{(revenue / (revenue / multiplier) - 1) * 100:.1f}%",
        "gross_margin": f"{gross_profit / revenue * 100:.1f}%",
        "operating_margin": f"{operating_profit / revenue * 100:.1f}%"
    }

    response = InterimReportResponse(
        company_id=request.company_id,
        period=request.period,
        currency="USD",
        revenue=current_revenue,
        comparative_revenue=comparative_revenue,
        balance_sheet=balance_sheet,
        comparative_balance_sheet=None,
        changes_in_equity=changes_in_equity,
        cash_flow=cash_flow,
        earnings_per_share=earnings_per_share,
        year_to_date=ytd_revenue_data,
        effective_tax_rate=income_tax / profit_before_tax if profit_before_tax > 0 else 0,
        management_commentary=commentary
    )

    logger.info("Interim report prepared", company=request.company_id, revenue=revenue)
    return response

def depreciation_calc(total_assets: float) -> float:
    """Calculate depreciation from total assets."""
    return total_assets * 0.35 * 0.1  # 10% depreciation rate on 35% of assets

@app.post("/year-to-date")
async def calculate_year_to_date(period: InterimPeriod):
    """Calculate year-to-date figures."""
    ytd_revenue = 2500000.0 * period.period_number
    ytd_profit = ytd_revenue * 0.12

    return {
        "year": period.year,
        "periods_included": period.period_number,
        "ytd_revenue": ytd_revenue,
        "ytd_profit": ytd_profit,
        "ytd_cash_flow": ytd_profit * 0.8
    }

@app.post("/effective-tax-rate")
async def calculate_effective_tax_rate(period: InterimPeriod):
    """Calculate effective tax rate for the period."""
    revenue = 2500000.0 * period.period_number
    profit_before_tax = revenue * 0.12
    income_tax_expense = profit_before_tax * 0.25

    return {
        "profit_before_tax": profit_before_tax,
        "income_tax_expense": income_tax_expense,
        "effective_tax_rate": 0.25,
        "standard_tax_rate": 0.25,
        "difference": 0.0
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8141)
