"""
Consolidated Financial Statements Service
Port: 8139
Consolidates parent and subsidiary financial statements with NCI and intercompany eliminations
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Consolidated Financial Statements Service", version="1.0.0")

# Pydantic Models
class SubsidiaryInput(BaseModel):
    subsidiary_id: str
    subsidiary_name: str
    ownership_percentage: float = Field(ge=0, le=100)
    functional_currency: str = "USD"
    acquisition_date: str
    goodwill: float = 0.0
    non_controlling_interests_percentage: float = 0.0

class IntercompanyTransaction(BaseModel):
    transaction_id: str
    transaction_type: str  # "sale", "loan", "dividend", "management_fee"
    parent_party: str
    subsidiary_party: str
    amount: float
    currency: str = "USD"
    transaction_date: str

class ConsolidationRequest(BaseModel):
    parent_company_id: str
    period_end: str
    subsidiaries: List[SubsidiaryInput]
    intercompany_transactions: List[IntercompanyTransaction]
    eliminate_intra_group_profits: bool = True
    eliminate_dividends: bool = True
    presentation_currency: str = "USD"
    include_non_controlling_interests: bool = True

class ConsolidatedRevenue(BaseModel):
    parent_revenue: float
    subsidiary_revenues: Dict[str, float]
    intercompany_sales_elimination: float
    consolidated_revenue: float

class ConsolidatedProfit(BaseModel):
    parent_profit: float
    subsidiary_profits: Dict[str, float]
    goodwill_amortization: float
    nci_share: float
    attributable_to_parent: float

class ConsolidatedBalanceSheet(BaseModel):
    total_assets: float
    non_current_assets: float
    current_assets: float
    total_liabilities: float
    non_current_liabilities: float
    current_liabilities: float
    equity_attributable_to_parent: float
    non_controlling_interests: float
    total_equity: float

class ConsolidatedCashFlow(BaseModel):
    operating_activities: float
    investing_activities: float
    financing_activities: float
    effect_of_exchange_rate: float
    net_increase_in_cash: float

class ConsolidationResponse(BaseModel):
    period: str
    consolidated_revenue: ConsolidatedRevenue
    consolidated_profit: ConsolidatedProfit
    consolidated_balance_sheet: ConsolidatedBalanceSheet
    consolidated_cash_flow: ConsolidatedCashFlow
    goodwill_recognized: float
    nci_at_fair_value: float

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal FinAcc service."""
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
    return {"status": "healthy", "service": "consolidated-financial-statements", "version": "1.0.0"}

@app.post("/consolidate", response_model=ConsolidationResponse)
async def consolidate_financial_statements(request: ConsolidationRequest):
    """Consolidate financial statements from parent and subsidiaries."""
    logger.info("Consolidating financial statements", parent=request.parent_company_id, period=request.period_end)

    # Simulate fetching financial data from accounting service
    parent_revenue = 10000000.0
    parent_profit = 1500000.0
    parent_assets = 50000000.0
    parent_liabilities = 30000000.0

    subsidiary_data = {}
    total_subsidiary_revenue = 0.0
    total_subsidiary_profit = 0.0
    total_subsidiary_assets = 0.0
    total_subsidiary_liabilities = 0.0

    for sub in request.subsidiaries:
        sub_revenue = 2000000.0 * (sub.ownership_percentage / 100)
        sub_profit = 300000.0 * (sub.ownership_percentage / 100)
        sub_assets = 5000000.0
        sub_liabilities = 2500000.0

        subsidiary_data[sub.subsidiary_id] = {
            "name": sub.subsidiary_name,
            "revenue": sub_revenue,
            "profit": sub_profit,
            "assets": sub_assets,
            "liabilities": sub_liabilities
        }

        total_subsidiary_revenue += sub_revenue
        total_subsidiary_profit += sub_profit
        total_subsidiary_assets += sub_assets
        total_subsidiary_liabilities += sub_liabilities

    # Calculate eliminations
    intercompany_sales = sum(t.amount for t in request.intercompany_transactions if t.transaction_type == "sale")
    intercompany_profit_elimination = intercompany_sales * 0.2  # 20% margin

    # NCI calculations
    nci_share_percentage = sum(sub.non_controlling_interests_percentage for sub in request.subsidiaries) / len(request.subsidiaries) if request.subsidiaries else 0
    nci_share = total_subsidiary_profit * (nci_share_percentage / 100)

    # Consolidated figures
    consolidated_revenue = ConsolidatedRevenue(
        parent_revenue=parent_revenue,
        subsidiary_revenues={k: v["revenue"] for k, v in subsidiary_data.items()},
        intercompany_sales_elimination=intercompany_sales,
        consolidated_revenue=parent_revenue + total_subsidiary_revenue - intercompany_sales
    )

    consolidated_profit = ConsolidatedProfit(
        parent_profit=parent_profit,
        subsidiary_profits={k: v["profit"] for k, v in subsidiary_data.items()},
        goodwill_amortization=50000.0,
        nci_share=nci_share,
        attributable_to_parent=parent_profit + total_subsidiary_profit - nci_share
    )

    consolidated_balance_sheet = ConsolidatedBalanceSheet(
        total_assets=parent_assets + total_subsidiary_assets,
        non_current_assets=(parent_assets + total_subsidiary_assets) * 0.6,
        current_assets=(parent_assets + total_subsidiary_assets) * 0.4,
        total_liabilities=parent_liabilities + total_subsidiary_liabilities,
        non_current_liabilities=(parent_liabilities + total_subsidiary_liabilities) * 0.4,
        current_liabilities=(parent_liabilities + total_subsidiary_liabilities) * 0.6,
        equity_attributable_to_parent=(parent_assets + total_subsidiary_assets) - (parent_liabilities + total_subsidiary_liabilities) - nci_share,
        non_controlling_interests=nci_share * 5,  # NCI at fair value
        total_equity=(parent_assets + total_subsidiary_assets) - (parent_liabilities + total_subsidiary_liabilities)
    )

    consolidated_cash_flow = ConsolidatedCashFlow(
        operating_activities=2000000.0,
        investing_activities=-800000.0,
        financing_activities=-500000.0,
        effect_of_exchange_rate=10000.0,
        net_increase_in_cash=710000.0
    )

    response = ConsolidationResponse(
        period=request.period_end,
        consolidated_revenue=consolidated_revenue,
        consolidated_profit=consolidated_profit,
        consolidated_balance_sheet=consolidated_balance_sheet,
        consolidated_cash_flow=consolidated_cash_flow,
        goodwill_recognized=sum(sub.goodwill for sub in request.subsidiaries),
        nci_at_fair_value=nci_share * 5
    )

    logger.info("Consolidation complete", consolidated_revenue=consolidated_revenue.consolidated_revenue)
    return response

@app.post("/eliminations")
async def calculate_eliminations(request: ConsolidationRequest):
    """Calculate intercompany eliminations."""
    eliminations = {
        "intercompany_sales": sum(t.amount for t in request.intercompany_transactions if t.transaction_type == "sale"),
        "intercompany_cost_of_sales": sum(t.amount * 0.7 for t in request.intercompany_transactions if t.transaction_type == "sale"),
        "intercompany_profit_remaining": sum(t.amount * 0.3 for t in request.intercompany_transactions if t.transaction_type == "sale"),
        "intercompany_dividends": sum(t.amount for t in request.intercompany_transactions if t.transaction_type == "dividend"),
        "intercompany_loans": sum(t.amount for t in request.intercompany_transactions if t.transaction_type == "loan"),
        "intercompany_management_fees": sum(t.amount for t in request.intercompany_transactions if t.transaction_type == "management_fee"),
        "total_eliminations": sum(t.amount for t in request.intercompany_transactions)
    }
    return eliminations

@app.post("/nci-calculation")
async def calculate_nci(request: ConsolidationRequest):
    """Calculate non-controlling interests."""
    total_subsidiary_profit = 500000.0  # Simulated
    total_subsidiary_equity = 2500000.0  # Simulated

    nci_data = {}
    for sub in request.subsidiaries:
        nci_pct = sub.non_controlling_interests_percentage
        nci_data[sub.subsidiary_id] = {
            "nci_percentage": nci_pct,
            "nci_profit_share": total_subsidiary_profit * (nci_pct / 100),
            "nci_equity_share": total_subsidiary_equity * (nci_pct / 100),
            "nci_fair_value_adjustment": total_subsidiary_equity * (nci_pct / 100) * 0.2
        }

    return {
        "subsidiary_nci_details": nci_data,
        "total_nci_profit": sum(d["nci_profit_share"] for d in nci_data.values()),
        "total_nci_equity": sum(d["nci_equity_share"] for d in nci_data.values())
    }

@app.post("/goodwill-calculation")
async def calculate_goodwill(request: ConsolidationRequest):
    """Calculate goodwill on acquisition."""
    goodwill_data = []
    total_goodwill = 0.0

    for sub in request.subsidiaries:
        consideration_paid = 6000000.0 * (sub.ownership_percentage / 100)
        fair_value_nci = 1500000.0 * (sub.non_controlling_interests_percentage / 100)
        net_identifiable_assets = 4500000.0

        goodwill = consideration_paid + fair_value_nci - net_identifiable_assets

        goodwill_data.append({
            "subsidiary_id": sub.subsidiary_id,
            "consideration_transferred": consideration_paid,
            "fair_value_nci": fair_value_nci,
            "net_identifiable_assets": net_identifiable_assets,
            "goodwill": goodwill if goodwill > 0 else 0
        })
        total_goodwill += goodwill if goodwill > 0 else 0

    return {
        "acquisitions": goodwill_data,
        "total_goodwill": total_goodwill,
        "impairment_test_required": total_goodwill > 0
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8139)
