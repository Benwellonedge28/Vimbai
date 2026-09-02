"""
Master Budget Service
Port: 8172
Comprehensive master budget including all functional budgets
"""

from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

logger = structlog.get_logger()
app = FastAPI(title="Master Budget Service", version="1.0.0")


class MasterBudgetRequest(BaseModel):
    company_id: str
    budget_year: str
    expected_sales_units: int
    selling_price_per_unit: float
    expected_production_units: int
    direct_material_cost_per_unit: float
    direct_labour_cost_per_unit: float
    variable_overhead_rate: float
    fixed_overhead: float
    operating_expenses: float


class MasterBudgetResponse(BaseModel):
    company_id: str
    budget_year: str
    sales_budget: Dict[str, float]
    production_budget: Dict[str, float]
    direct_materials_budget: Dict[str, float]
    direct_labour_budget: Dict[str, float]
    overhead_budget: Dict[str, float]
    budgeted_income_statement: Dict[str, float]
    budgeted_balance_sheet: Dict[str, float]
    budgeted_cash_flow: Dict[str, float]
    expected_profit: float


async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "master-budget", "version": "1.0.0"}


@app.post("/prepare", response_model=MasterBudgetResponse)
async def prepare_master_budget(request: MasterBudgetRequest):
    logger.info("Preparing master budget", company=request.company_id, year=request.budget_year)

    sales_budget = {
        "units": request.expected_sales_units,
        "revenue": request.expected_sales_units * request.selling_price_per_unit,
    }

    production_budget = {
        "units_to_produce": request.expected_production_units,
        "opening_inventory": 1000,
        "closing_inventory": 1000,
    }

    direct_materials = {
        "cost_per_unit": request.direct_material_cost_per_unit,
        "total_cost": request.expected_production_units * request.direct_material_cost_per_unit,
    }

    direct_labour = {
        "cost_per_unit": request.direct_labour_cost_per_unit,
        "total_cost": request.expected_production_units * request.direct_labour_cost_per_unit,
    }

    overhead = {
        "variable": request.expected_production_units * request.variable_overhead_rate,
        "fixed": request.fixed_overhead,
        "total": request.expected_production_units * request.variable_overhead_rate + request.fixed_overhead,
    }

    cost_of_goods_sold = direct_materials["total_cost"] + direct_labour["total_cost"] + overhead["total"]
    gross_profit = sales_budget["revenue"] - cost_of_goods_sold

    income_statement = {
        "revenue": sales_budget["revenue"],
        "cost_of_goods_sold": cost_of_goods_sold,
        "gross_profit": gross_profit,
        "operating_expenses": request.operating_expenses,
        "operating_profit": gross_profit - request.operating_expenses,
    }

    return MasterBudgetResponse(
        company_id=request.company_id,
        budget_year=request.budget_year,
        sales_budget=sales_budget,
        production_budget=production_budget,
        direct_materials_budget=direct_materials,
        direct_labour_budget=direct_labour,
        overhead_budget=overhead,
        budgeted_income_statement=income_statement,
        budgeted_balance_sheet={"total_assets": 1000000, "total_equity": 600000},
        budgeted_cash_flow={"operating": income_statement["operating_profit"], "financing": 0},
        expected_profit=income_statement["operating_profit"],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8172)
