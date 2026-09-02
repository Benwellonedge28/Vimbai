"""
Vimbai Sales Price Variance Service
Calculates sales price and volume variances for revenue analysis.
Port: 8341
"""
import os, uuid
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "sales-price-variance-service"
PORT = int(os.getenv("PORT", "8341"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Sales Price Variance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class VarianceItem(BaseModel):
    product_name: str; budgeted_price: float; actual_price: float
    budgeted_volume: float; actual_volume: float

class VarianceRequest(BaseModel):
    company_id: str; period: str; items: List[VarianceItem]

class VarianceResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    total_sales_price_variance: float; total_sales_volume_variance: float
    total_sales_mix_variance: float; total_sales_quantity_variance: float
    items: List[Dict] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/analyze", response_model=VarianceResult)
async def analyze_variance(req: VarianceRequest):
    total_price_var = 0; total_volume_var = 0
    total_mix_var = 0; total_qty_var = 0
    items_result = []
    
    total_budget_volume = sum(i.budgeted_volume for i in req.items)
    total_actual_volume = sum(i.actual_volume for i in req.items)
    avg_budget_price = sum(i.budgeted_price * i.budgeted_volume for i in req.items) / total_budget_volume if total_budget_volume else 0
    volume_ratio = total_actual_volume / total_budget_volume if total_budget_volume else 0
    
    for item in req.items:
        price_variance = (item.actual_price - item.budgeted_price) * item.actual_volume
        volume_variance = (item.actual_volume - item.budgeted_volume) * item.budgeted_price
        
        revised_budget_volume = item.budgeted_volume * volume_ratio
        mix_variance = (item.actual_volume - revised_budget_volume) * item.budgeted_price
        quantity_variance = (revised_budget_volume - item.budgeted_volume) * item.budgeted_price
        
        total_price_var += price_variance
        total_volume_var += volume_variance
        total_mix_var += mix_variance
        total_qty_var += quantity_variance
        
        items_result.append({
            "product": item.product_name,
            "sales_price_variance": round(price_variance, 2),
            "sales_volume_variance": round(volume_variance, 2),
            "sales_mix_variance": round(mix_variance, 2),
            "sales_quantity_variance": round(quantity_variance, 2),
            "favorable": price_variance >= 0 and volume_variance >= 0
        })
    
    return VarianceResult(
        company_id=req.company_id, period=req.period,
        total_sales_price_variance=round(total_price_var, 2),
        total_sales_volume_variance=round(total_volume_var, 2),
        total_sales_mix_variance=round(total_mix_var, 2),
        total_sales_quantity_variance=round(total_qty_var, 2),
        items=items_result
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
