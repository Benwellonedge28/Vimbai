"""
Vimbai Tax Calculation Service
Handles VAT, Income Tax, and other tax calculations.
"""

import os
import uuid
from datetime import datetime
from typing:Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "tax-calculation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8137"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Tax Calculation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


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


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Tax calculation services"}


@app.post("/vat/calculate")
async def calculate_vat(amount: float, vat_rate: float, is_inclusive: bool = False):
    """
    Calculate VAT.
    If inclusive: VAT = Amount × (Rate / (100 + Rate))
    If exclusive: VAT = Amount × (Rate / 100)
    """
    if is_inclusive:
        vat = amount * (vat_rate / (100 + vat_rate))
        net_amount = amount - vat
    else:
        vat = amount * (vat_rate / 100)
        net_amount = amount
        gross_amount = amount + vat

    return {
        "net_amount": round(net_amount, 2),
        "vat_rate": vat_rate,
        "vat_amount": round(vat, 2),
        "gross_amount": round(net_amount + vat if not is_inclusive else amount, 2),
        "vat_inclusive": is_inclusive
    }


@app.post("/vat/quarterly-return")
async def quarterly_vat_return(
    quarter: str,
    taxable_supplies: float,
    vat_on_supplies: float,
    vat_on_purchases: float,
    vat_refunds: float = 0
):
    """Calculate quarterly VAT return."""
    output_vat = vat_on_supplies
    input_vat = vat_on_purchases + vat_refunds
    vat_payable = output_vat - input_vat

    return {
        "quarter": quarter,
        "taxable_supplies": taxable_supplies,
        "output_vat": output_vat,
        "input_vat": input_vat,
        "vat_refunds": vat_refunds,
        "vat_payable": round(vat_payable, 2),
        "status": "Payable to tax authority" if vat_payable > 0 else "Refundable"
    }


@app.post("/income-tax/calculate")
async def calculate_income_tax(
    taxable_income: float,
    tax_brackets: List[dict] = None  # [{"min": 0, "max": 50000, "rate": 0}, ...]
):
    """Calculate progressive income tax."""
    if tax_brackets is None:
        # Default UK-style brackets
        tax_brackets = [
            {"min": 0, "max": 12500, "rate": 0},
            {"min": 12500, "max": 50000, "rate": 20},
            {"min": 50000, "max": 150000, "rate": 40},
            {"min": 150000, "max": float('inf'), "rate": 45}
        ]

    total_tax = 0
    breakdown = []

    remaining_income = taxable_income

    for bracket in tax_brackets:
        min_income = bracket["min"]
        max_income = bracket["max"]
        rate = bracket["rate"]

        if remaining_income <= 0:
            break

        taxable_in_bracket = min(remaining_income, max_income - min_income)
        if taxable_in_bracket > 0:
            tax_in_bracket = taxable_in_bracket * (rate / 100)
            total_tax += tax_in_bracket
            breakdown.append({
                "bracket": f"{min_income} - {max_income}",
                "rate": rate,
                "taxable_amount": taxable_in_bracket,
                "tax": round(tax_in_bracket, 2)
            })
            remaining_income -= taxable_in_bracket

    effective_rate = (total_tax / taxable_income * 100) if taxable_income != 0 else 0

    return {
        "taxable_income": taxable_income,
        "tax_brackets": breakdown,
        "total_tax": round(total_tax, 2),
        "effective_tax_rate": round(effective_rate, 2)
    }


@app.post("/corporation-tax")
async def calculate_corporation_tax(
    profits_before_tax: float,
    capital_allowances: float = 0,
    brought_forward_losses: float = 0,
    tax_rate: float = 19
):
    """Calculate corporation tax."""
    adjusted_profits = profits_before_tax - capital_allowances
    taxable_profits = max(adjusted_profits - brought_forward_losses, 0)
    tax = taxable_profits * (tax_rate / 100)
    effective_rate = (tax / taxable_profits * 100) if taxable_profits != 0 else 0

    return {
        "profits_before_tax": profits_before_tax,
        "capital_allowances": capital_allowances,
        "brought_forward_losses": brought_forward_losses,
        "adjusted_profits": adjusted_profits,
        "taxable_profits": taxable_profits,
        "tax_rate": tax_rate,
        "corporation_tax": round(tax, 2),
        "effective_rate": round(effective_rate, 2)
    }


@app.post("/capital-gains-tax")
async def calculate_capital_gains_tax(
    disposal_proceeds: float,
    cost_basis: float,
    allowable_expenses: float = 0,
    annual_exemption: float = 12300,
    tax_rate: float = 20
):
    """Calculate Capital Gains Tax."""
    gain = disposal_proceeds - cost_basis - allowable_expenses
    taxable_gain = max(gain - annual_exemption, 0)
    cgt = taxable_gain * (tax_rate / 100)

    return {
        "disposal_proceeds": disposal_proceeds,
        "cost_basis": cost_basis,
        "allowable_expenses": allowable_expenses,
        "gross_gain": gain,
        "annual_exemption": annual_exemption,
        "taxable_gain": taxable_gain,
        "cgt_rate": tax_rate,
        "capital_gains_tax": round(cgt, 2)
    }


@app.post("/withholding-tax")
async def calculate_withholding_tax(
    gross_payment: float,
    withholding_rate: float
):
    """Calculate withholding tax on payments."""
    tax_withheld = gross_payment * (withholding_rate / 100)
    net_payment = gross_payment - tax_withheld

    return {
        "gross_payment": gross_payment,
        "withholding_rate": withholding_rate,
        "tax_withheld": round(tax_withheld, 2),
        "net_payment": round(net_payment, 2)
    }


@app.post("/tax-on-dividends")
async def calculate_dividend_tax(
    dividend_amount: float,
    tax_credits: float = 0,
    personal_allowance: float = 2000,
    basic_rate: float = 7.5,
    higher_rate: float = 32.5,
    additional_rate: float = 38.1
):
    """Calculate tax on dividends (UK-style)."""
    taxable_dividend = max(dividend_amount - personal_allowance, 0)
    gross_dividend = dividend_amount + tax_credits

    # Simplified calculation
    tax = taxable_dividend * (basic_rate / 100)

    return {
        "dividend_amount": dividend_amount,
        "tax_credits": tax_credits,
        "personal_allowance": personal_allowance,
        "taxable_dividend": taxable_dividend,
        "dividend_tax": round(tax, 2)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
