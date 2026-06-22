"""
Foreign Currency Translation Service
Port: 8143
Translates foreign subsidiary statements using current rate or temporal method
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Foreign Currency Translation Service", version="1.0.0")

# Pydantic Models
class ExchangeRates(BaseModel):
    spot_rate: float
    average_rate: float
    historical_rates: Dict[str, float]
    reporting_date: str

class SubsidiaryStatement(BaseModel):
    currency: str
    items: Dict[str, float]
    statement_date: str
    revenue: float
    cost_of_sales: float
    operating_expenses: float
    net_profit: float
    property_plant_equipment: float
    inventory: float
    cash: float
    receivables: float
    payables: float
    borrowings: float
    share_capital: float
    retained_earnings: float

class TranslationRequest(BaseModel):
    subsidiary_id: str
    functional_currency: str
    presentation_currency: str
    translation_method: str = Field(pattern="^(current_rate|temporal)$")
    statement_date: str
    exchange_rates: ExchangeRates
    subsidiary_statement: SubsidiaryStatement
    include_cumulative_translation_adjustment: bool = True

class TranslatedStatement(BaseModel):
    currency: str
    translated_items: Dict[str, float]
    translated_revenue: float
    translated_cost_of_sales: float
    translated_operating_expenses: float
    translated_net_profit: float
    translated_assets: Dict[str, float]
    translated_liabilities: Dict[str, float]
    translated_equity: Dict[str, float]
    cumulative_translation_adjustment: float
    total_assets: float
    total_liabilities: float
    total_equity: float

class TranslationResponse(BaseModel):
    subsidiary_id: str
    functional_currency: str
    presentation_currency: str
    translation_method: str
    exchange_rates_used: ExchangeRates
    translated_income_statement: TranslatedStatement
    translated_balance_sheet: TranslatedStatement
    translation_gain_loss: float

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
    return {"status": "healthy", "service": "foreign-currency-translation", "version": "1.0.0"}

@app.post("/translate", response_model=TranslationResponse)
async def translate_financial_statements(request: TranslationRequest):
    """Translate foreign currency financial statements."""
    logger.info("Translating statements", subsidiary=request.subsidiary_id, method=request.translation_method)

    rates = request.exchange_rates
    statement = request.subsidiary_statement

    if request.translation_method == "current_rate":
        # Current rate method (all assets/liabilities at closing rate)
        # Income statement at average rate

        translated_revenue = statement.revenue * rates.average_rate
        translated_cos = statement.cost_of_sales * rates.average_rate
        translated_op_exp = statement.operating_expenses * rates.average_rate
        translated_net_profit = statement.net_profit * rates.average_rate

        translated_ppe = statement.property_plant_equipment * rates.spot_rate
        translated_inventory = statement.inventory * rates.spot_rate
        translated_cash = statement.cash * rates.spot_rate
        translated_receivables = statement.receivables * rates.spot_rate
        translated_payables = statement.payables * rates.spot_rate
        translated_borrowings = statement.borrowings * rates.spot_rate
        translated_capital = statement.share_capital * rates.spot_rate  # Historical rate
        translated_retained = statement.retained_earnings * rates.spot_rate

        # Calculate CTA
        net_assets = statement.property_plant_equipment + statement.inventory + statement.cash + statement.receivables - statement.payables - statement.borrowings
        translated_net_assets = translated_ppe + translated_inventory + translated_cash + translated_receivables - translated_payables - translated_borrowings
        cta = translated_net_assets - (net_assets * rates.spot_rate) - translated_retained + (statement.retained_earnings * rates.average_rate)

        translated_equity = translated_capital + translated_retained + cta
        translated_liabilities = translated_payables + translated_borrowings
        translated_assets = translated_ppe + translated_inventory + translated_cash + translated_receivables

    else:  # Temporal method
        # Monetary items at closing rate, non-monetary at historical rate
        historical_rate = rates.historical_rates.get("historical", rates.average_rate * 0.95)

        translated_revenue = statement.revenue * rates.average_rate
        translated_cos = statement.cost_of_sales * rates.average_rate
        translated_op_exp = statement.operating_expenses * rates.average_rate
        translated_net_profit = statement.net_profit * rates.average_rate

        translated_ppe = statement.property_plant_equipment * historical_rate
        translated_inventory = statement.inventory * rates.spot_rate  # Lower of cost or NRV
        translated_cash = statement.cash * rates.spot_rate
        translated_receivables = statement.receivables * rates.spot_rate
        translated_payables = statement.payables * rates.spot_rate
        translated_borrowings = statement.borrowings * rates.spot_rate
        translated_capital = statement.share_capital * historical_rate
        translated_retained = statement.retained_earnings * historical_rate

        # CTA = 0 for temporal method, translation gain/loss goes to P&L
        cta = 0.0
        translation_gain_loss = translated_net_profit - statement.net_profit * rates.spot_rate

        translated_equity = translated_capital + translated_retained
        translated_liabilities = translated_payables + translated_borrowings
        translated_assets = translated_ppe + translated_inventory + translated_cash + translated_receivables

    total_assets = translated_ppe + translated_inventory + translated_cash + translated_receivables
    total_liabilities = translated_payables + translated_borrowings
    total_equity = translated_equity

    translated_income = TranslatedStatement(
        currency=request.presentation_currency,
        translated_items={},
        translated_revenue=translated_revenue,
        translated_cost_of_sales=translated_cos,
        translated_operating_expenses=translated_op_exp,
        translated_net_profit=translated_net_profit,
        translated_assets={},
        translated_liabilities={},
        translated_equity={},
        cumulative_translation_adjustment=0.0,
        total_assets=translated_revenue,
        total_liabilities=translated_cos + translated_op_exp,
        total_equity=translated_net_profit
    )

    translated_bs = TranslatedStatement(
        currency=request.presentation_currency,
        translated_items={},
        translated_revenue=0,
        translated_cost_of_sales=0,
        translated_operating_expenses=0,
        translated_net_profit=0,
        translated_assets={
            "property_plant_equipment": translated_ppe,
            "inventory": translated_inventory,
            "cash": translated_cash,
            "receivables": translated_receivables
        },
        translated_liabilities={
            "payables": translated_payables,
            "borrowings": translated_borrowings
        },
        translated_equity={
            "share_capital": translated_capital,
            "retained_earnings": translated_retained,
            "cumulative_translation_adjustment": cta
        },
        cumulative_translation_adjustment=cta,
        total_assets=total_assets,
        total_liabilities=total_liabilities,
        total_equity=total_equity
    )

    response = TranslationResponse(
        subsidiary_id=request.subsidiary_id,
        functional_currency=request.functional_currency,
        presentation_currency=request.presentation_currency,
        translation_method=request.translation_method,
        exchange_rates_used=rates,
        translated_income_statement=translated_income,
        translated_balance_sheet=translated_bs,
        translation_gain_loss=translated_net_profit - statement.net_profit * rates.spot_rate if request.translation_method == "temporal" else cta
    )

    logger.info("Translation complete", subsidiary=request.subsidiary_id, cta=cta)
    return response

@app.post("/exchange-rate-conversion")
async def convert_amount(amount: float, from_currency: str, to_currency: str, rate: float):
    """Convert amount from one currency to another."""
    converted = amount * rate
    return {
        "original_amount": amount,
        "original_currency": from_currency,
        "converted_amount": converted,
        "target_currency": to_currency,
        "exchange_rate": rate
    }

@app.post("/closing-rate")
async def calculate_closing_rate(functional_amount: float, closing_rate: float, historical_rate: float):
    """Calculate translation difference using closing rate method."""
    translated = functional_amount * closing_rate
    historical_translated = functional_amount * historical_rate
    translation_difference = translated - historical_translated

    return {
        "functional_amount": functional_amount,
        "closing_rate": closing_rate,
        "historical_rate": historical_rate,
        "translated_at_closing": translated,
        "translated_at_historical": historical_translated,
        "translation_difference": translation_difference
    }

@app.post("/net-investment")
async def calculate_net_investment(assets: List[float], liabilities: List[List[str]], functional_rate: float):
    """Calculate net investment in foreign operation."""
    total_assets = sum(assets)
    total_liabilities = sum(sum(l) if isinstance(l, list) else l for l in liabilities)
    net_investment = total_assets - total_liabilities
    net_investment_functional = net_investment * functional_rate

    return {
        "total_assets": total_assets,
        "total_liabilities": total_liabilities,
        "net_investment_functional": net_investment,
        "net_investment_presentation": net_investment_functional
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8143)
