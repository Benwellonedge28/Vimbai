"""
FinAcc Working Capital Service
Manages working capital and liquidity analysis.
"""

import os
import uuid
from datetime: datetime
from typing:Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "working-capital-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8136"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Working Capital Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


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


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Working capital management"}


@app.post("/calculate")
async def calculate_working_capital(
    current_assets: float,
    current_liabilities: float
):
    """Calculate Net Working Capital."""
    nwc = current_assets - current_liabilities

    return {
        "current_assets": current_assets,
        "current_liabilities": current_liabilities,
        "net_working_capital": nwc,
        "interpretation": "Positive - Healthy" if nwc > 0 else "Negative - Liquidity Crisis"
    }


@app.post("/operating-cycle")
async def calculate_operating_cycle(
    inventory_days: float,
    debtors_days: float,
    creditors_days: float
):
    """
    Calculate Cash Conversion Cycle (Operating Cycle).
    CCC = Inventory Days + Debtors Days - Creditors Days
    """
    cash_conversion_cycle = inventory_days + debtors_days - creditors_days

    return {
        "inventory_days": inventory_days,
        "debtors_days": debtors_days,
        "creditors_days": creditors_days,
        "cash_conversion_cycle": round(cash_conversion_cycle, 1),
        "interpretation": "Faster cycle" if cash_conversion_cycle < 90 else "Slower cycle",
        "recommendation": "Reduce inventory/days debtors OR extend creditors" if cash_conversion_cycle > 90 else "Maintain current practices"
    }


@app.post("/working-capital-ratio")
async def working_capital_ratio(
    working_capital: float,
    total_assets: float
):
    """Calculate Working Capital to Total Assets ratio."""
    ratio = (working_capital / total_assets) if total_assets != 0 else 0
    return {
        "working_capital": working_capital,
        "total_assets": total_assets,
        "wc_to_assets_ratio": round(ratio, 4),
        "interpretation": "Adequate" if ratio >= 0.2 else "Low working capital"
    }


@app.post("/current-ratio-analysis")
async def current_ratio_analysis(
    current_assets: float,
    current_liabilities: float,
    industry_average: float = 1.5
):
    """Analyze current ratio vs industry benchmark."""
    ratio = current_assets / current_liabilities if current_liabilities != 0 else 0
    deviation = ratio - industry_average

    return {
        "current_ratio": round(ratio, 2),
        "industry_average": industry_average,
        "deviation": round(deviation, 2),
        "status": "Above benchmark" if ratio > industry_average else "Below benchmark",
        "action": "Consider improving liquidity" if ratio < industry_average else "Maintain current position"
    }


@app.post("/optimum-working-capital")
async def calculate_optimum_working_capital(
    annual_revenue: float,
    current_ratio_target: float = 1.5
):
    """Calculate optimum working capital level."""
    optimum_current_liabilities = annual_revenue / 4  # Assume CL = 25% of annual revenue
    optimum_current_assets = optimum_current_liabilities * current_ratio_target
    optimum_nwc = optimum_current_assets - optimum_current_liabilities

    return {
        "annual_revenue": annual_revenue,
        "target_current_ratio": current_ratio_target,
        "optimum_current_liabilities": round(optimum_current_liabilities, 2),
        "optimum_current_assets": round(optimum_current_assets, 2),
        "optimum_nwc": round(optimum_nwc, 2)
    }


@app.post("/debtor-collection")
async def debtor_collection_analysis(
    opening_debtors: float,
    closing_debtors: float,
    credit_sales: float
):
    """Analyze debtor collection."""
    avg_debtors = (opening_debtors + closing_debtors) / 2
    debtors_turnover = credit_sales / avg_debtors if avg_debtors != 0 else 0
    collection_period = 365 / debtors_turnover if debtors_turnover != 0 else 0

    return {
        "opening_debtors": opening_debtors,
        "closing_debtors": closing_debtors,
        "average_debtors": avg_debtors,
        "credit_sales": credit_sales,
        "debtors_turnover": round(debtors_turnover, 2),
        "collection_period_days": round(collection_period, 1),
        "recommendation": "Review credit policy" if collection_period > 60 else "Collection is healthy"
    }


@app.post("/creditor-analysis")
async def creditor_payment_analysis(
    opening_creditors: float,
    closing_creditors: float,
    credit_purchases: float
):
    """Analyze creditor payment period."""
    avg_creditors = (opening_creditors + closing_creditors) / 2
    creditors_turnover = credit_purchases / avg_creditors if avg_creditors != 0 else 0
    payment_period = 365 / creditors_turnover if creditors_turnover != 0 else 0

    return {
        "opening_creditors": opening_creditors,
        "closing_creditors": closing_creditors,
        "average_creditors": avg_creditors,
        "credit_purchases": credit_purchases,
        "creditors_turnover": round(creditors_turnover, 2),
        "payment_period_days": round(payment_period, 1),
        "recommendation": "Extend payment period" if payment_period < 30 else "Payment terms are healthy"
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
