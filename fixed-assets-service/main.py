"""
Fixed Assets Service
Port: 8329
Fixed assets management and depreciation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Fixed Assets Service", version="1.0.0")

class FixedAsset(BaseModel):
    asset_id: str
    asset_name: str
    category: str
    purchase_date: str
    purchase_cost: float
    salvage_value: float
    useful_life_years: int
    depreciation_method: str
    accumulated_depreciation: float

class FixedAssetsRequest(BaseModel):
    company_id: str
    assets: List[FixedAsset]
    capital_budget: float
    target_capitalization_rate: float

class FixedAssetsResponse(BaseModel):
    company_id: str
    asset_summary: Dict[str, Any]
    depreciation_schedule: List[Dict[str, Any]]
    asset_category_analysis: List[Dict[str, Any]]
    capital_expenditure_plan: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "fixed-assets", "version": "1.0.0"}

@app.post("/analyze", response_model=FixedAssetsResponse)
async def analyze_fixed_assets(request: FixedAssetsRequest):
    logger.info("Analyzing fixed assets", company=request.company_id)
    
    total_cost = sum(a.purchase_cost for a in request.assets)
    total_accumulated = sum(a.accumulated_depreciation for a in request.assets)
    net_book_value = total_cost - total_accumulated
    
    depreciation_schedule = []
    for asset in request.assets[:10]:
        annual_dep = (asset.purchase_cost - asset.salvage_value) / asset.useful_life_years
        remaining_life = max(0, asset.useful_life_years - asset.accumulated_depreciation / annual_dep if annual_dep else 0)
        depreciation_schedule.append({
            "asset_id": asset.asset_id,
            "asset_name": asset.asset_name,
            "net_book_value": round(asset.purchase_cost - asset.accumulated_depreciation, 2),
            "remaining_life_years": round(remaining_life, 2),
            "annual_depreciation": round(annual_dep, 2)
        })
    
    category_totals = {}
    for asset in request.assets:
        if asset.category not in category_totals:
            category_totals[asset.category] = {"cost": 0, "accumulated": 0}
        category_totals[asset.category]["cost"] += asset.purchase_cost
        category_totals[asset.category]["accumulated"] += asset.accumulated_depreciation
    
    asset_category_analysis = [
        {"category": k, "total_cost": round(v["cost"], 2), "accumulated_dep": round(v["accumulated"], 2), "net_value": round(v["cost"] - v["accumulated"], 2)}
        for k, v in category_totals.items()
    ]
    
    capital_expenditure_plan = {
        "capital_budget": request.capital_budget,
        "replacement_needs": round(total_cost * 0.1, 2),
        "expansion_allocations": round(request.capital_budget * 0.3, 2)
    }
    
    recommendations = []
    fully_depreciated = sum(1 for a in request.assets if a.purchase_cost - a.accumulated_depreciation <= a.salvage_value)
    if fully_depreciated > 0:
        recommendations.append(f"{fully_depreciated} fully depreciated assets need replacement")
    if total_accumulated / total_cost > 0.7:
        recommendations.append("High accumulated depreciation - plan for asset renewal")

    return FixedAssetsResponse(
        company_id=request.company_id,
        asset_summary={"total_assets": len(request.assets), "total_cost": round(total_cost, 2), "net_book_value": round(net_book_value, 2)},
        depreciation_schedule=depreciation_schedule,
        asset_category_analysis=asset_category_analysis,
        capital_expenditure_plan=capital_expenditure_plan,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8329)
