"""
Inventory Valuation Service
Port: 8377
Inventory costing methods and valuation
"""
import httpx
import structlog
from typing import Any, Dict, List
from datetime import datetime
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Inventory Valuation Service", version="1.0.0")

class InventoryValuationRequest(BaseModel):
    company_id: str
    method: str
    purchases: List[Dict[str, Any]]
    sales: List[Dict[str, Any]]

class InventoryValuationResponse(BaseModel):
    company_id: str
    method: str
    ending_inventory: float
    cost_of_goods_sold: float
    gross_profit: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "inventory-valuation", "version": "1.0.0"}

@app.post("/calculate", response_model=InventoryValuationResponse)
async def calculate_inventory(request: InventoryValuationRequest):
    logger.info("Calculating inventory", company=request.company_id, method=request.method)
    
    total_purchases = sum(p.get("quantity", 0) * p.get("unit_cost", 0) for p in request.purchases)
    total_sales = sum(s.get("quantity", 0) * s.get("unit_price", 0) for s in request.sales)
    cogs = total_purchases * 0.7
    ending_inv = total_purchases - cogs
    
    return InventoryValuationResponse(
        company_id=request.company_id,
        method=request.method,
        ending_inventory=round(ending_inv, 2),
        cost_of_goods_sold=round(cogs, 2),
        gross_profit=round(total_sales - cogs, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8377)
