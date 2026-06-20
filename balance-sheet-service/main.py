"""
FinAcc Balance Sheet Service
Generates Statement of Financial Position (Balance Sheet).
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

SERVICE_NAME = "balance-sheet-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8135"))
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdstdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Balance Sheet Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Balance Sheet / Statement of Financial Position"}


@app.post("/generate")
async def generate_balance_sheet(
    # Non-Current Assets
    land_and_buildings: float = 0,
    plant_and_machinery: float = 0,
    fixtures_and_fittings: float = 0,
    motor_vehicles: float = 0,
    accumulated_depreciation: float = 0,
    # Current Assets
    inventory: float = 0,
    trade_debtors: float = 0,
    other_receivables: float = 0,
    cash_and_bank: float = 0,
    # Equity
    share_capital: float = 0,
    retained_earnings: float = 0,
    revaluation_reserve: float = 0,
    # Non-Current Liabilities
    long_term_borrowings: float = 0,
    debentures: float = 0,
    # Current Liabilities
    trade_creditors: float = 0,
    short_term_borrowings: float = 0,
    accruals: float = 0,
    taxation: float = 0,
    dividends_payable: float = 0
):
    """
    Generate Balance Sheet.
    Assets = Equity + Liabilities
    """
    # Calculate Net Non-Current Assets
    total_nca = land_and_buildings + plant_and_machinery + fixtures_and_fittings + motor_vehicles - accumulated_depreciation

    # Calculate Total Assets
    total_current_assets = inventory + trade_debtors + other_receivables + cash_and_bank
    total_assets = total_nca + total_current_assets

    # Calculate Equity
    total_equity = share_capital + retained_earnings + revaluation_reserve

    # Calculate Non-Current Liabilities
    total_ncl = long_term_borrowings + debentures

    # Calculate Current Liabilities
    total_cl = trade_creditors + short_term_borrowings + accruals + taxation + dividends_payable
    total_liabilities = total_ncl + total_cl

    # Balance Check
    total_liabilities_and_equity = total_equity + total_liabilities
    is_balanced = abs(total_assets - total_liabilities_and_equity) < 0.01

    return {
        "balance_sheet": {
            "assets": {
                "non_current_assets": {
                    "land_and_buildings": land_and_buildings,
                    "plant_and_machinery": plant_and_machinery,
                    "fixtures_and_fittings": fixtures_and_fittings,
                    "motor_vehicles": motor_vehicles,
                    "less_accumulated_depreciation": accumulated_depreciation,
                    "total_nca": total_nca
                },
                "current_assets": {
                    "inventory": inventory,
                    "trade_debtors": trade_debtors,
                    "other_receivables": other_receivables,
                    "cash_and_bank": cash_and_bank,
                    "total_current_assets": total_current_assets
                },
                "total_assets": total_assets
            },
            "equity_and_liabilities": {
                "equity": {
                    "share_capital": share_capital,
                    "retained_earnings": retained_earnings,
                    "revaluation_reserve": revaluation_reserve,
                    "total_equity": total_equity
                },
                "non_current_liabilities": {
                    "long_term_borrowings": long_term_borrowings,
                    "debentures": debentures,
                    "total_ncl": total_ncl
                },
                "current_liabilities": {
                    "trade_creditors": trade_creditors,
                    "short_term_borrowings": short_term_borrowings,
                    "accruals": accruals,
                    "taxation": taxation,
                    "dividends_payable": dividends_payable,
                    "total_cl": total_cl
                },
                "total_equity_and_liabilities": total_liabilities_and_equity
            }
        },
        "balance_check": {
            "total_assets": total_assets,
            "total_equity_and_liabilities": total_liabilities_and_equity,
            "is_balanced": is_balanced,
            "difference": round(total_assets - total_liabilities_and_equity, 2)
        }
    }


@app.post("/vertical-format")
async def vertical_format_balance_sheet(
    total_assets: float,
    total_liabilities: float,
    share_capital: float,
    reserves: float = 0,
    revaluation_reserve: float = 0
):
    """Generate Vertical Format Balance Sheet."""
    equity = share_capital + reserves + revaluation_reserve

    return {
        "assets Employed": {
            "non_current_assets": "See detailed breakdown",
            "current_assets": "See detailed breakdown",
            "total_assets": total_assets
        },
        "financed_by": {
            "capital_and_reserves": {
                "share_capital": share_capital,
                "reserves": reserves,
                "revaluation_reserve": revaluation_reserve,
                "total": equity
            },
            "non_current_liabilities": "See detailed breakdown",
            "current_liabilities": "See detailed breakdown",
            "total_liabilities": total_liabilities
        },
        "total": total_assets,
        "check": f"Total Assets ({total_assets}) = Equity ({equity}) + Liabilities ({total_liabilities})"
    }


@app.post("/ratio-analysis")
async def balance_sheet_ratios(
    current_assets: float,
    current_liabilities: float,
    inventory: float,
    cash: float,
    total_debt: float,
    total_equity: float,
    current_assets_detail: dict = {},
    current_liabilities_detail: dict = {}
):
    """Calculate key balance sheet ratios."""
    current_ratio = current_assets / current_liabilities if current_liabilities != 0 else 0
    quick_ratio = (current_assets - inventory) / current_liabilities if current_liabilities != 0 else 0
    cash_ratio = cash / current_liabilities if current_liabilities != 0 else 0
    debt_equity = total_debt / total_equity if total_equity != 0 else 0
    debt_ratio = total_debt / (current_assets + (current_assets - inventory)) if current_assets != 0 else 0

    return {
        "liquidity_ratios": {
            "current_ratio": round(current_ratio, 2),
            "quick_ratio": round(quick_ratio, 2),
            "cash_ratio": round(cash_ratio, 2)
        },
        "leverage_ratios": {
            "debt_equity_ratio": round(debt_equity, 2),
            "debt_ratio": round(debt_ratio, 4)
        },
        "interpretation": {
            "current_ratio": "Adequate" if current_ratio >= 1.5 else "Needs improvement",
            "quick_ratio": "Adequate" if quick_ratio >= 1 else "Low liquidity",
            "debt_equity": "Healthy" if debt_equity <= 2 else "High leverage"
        }
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
