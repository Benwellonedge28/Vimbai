"""
Vimbai Sales Volume Variance Service
Analyzes volume variances broken into mix and quantity components.
Port: 8342
"""
import os, uuid
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "sales-volume-variance-service"
PORT = int(os.getenv("PORT", "8342"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Sales Volume Variance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class VolumeItem(BaseModel):
    product_name: str; budgeted_volume: float; actual_volume: float
    budgeted_price: float; standard_mix_pct: float = 0

class VolumeRequest(BaseModel):
    company_id: str; period: str; items: List[VolumeItem]

class VolumeResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    total_volume_variance: float; total_mix_variance: float; total_quantity_variance: float
    items: List[Dict] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/analyze", response_model=VolumeResult)
async def analyze_volume_variance(req: VolumeRequest):
    total_budget_vol = sum(i.budgeted_volume for i in req.items)
    total_actual_vol = sum(i.actual_volume for i in req.items)
    volume_ratio = total_actual_vol / total_budget_vol if total_budget_vol else 0
    
    total_vol_var = 0; total_mix = 0; total_qty = 0
    items_result = []
    
    for item in req.items:
        revised_budget = item.budgeted_volume * volume_ratio
        vol_variance = (item.actual_volume - item.budgeted_volume) * item.budgeted_price
        mix_variance = (item.actual_volume - revised_budget) * item.budgeted_price
        qty_variance = (revised_budget - item.budgeted_volume) * item.budgeted_price
        
        total_vol_var += vol_variance; total_mix += mix_variance; total_qty += qty_variance
        items_result.append({
            "product": item.product_name,
            "volume_variance": round(vol_variance, 2),
            "mix_variance": round(mix_variance, 2),
            "quantity_variance": round(qty_variance, 2),
            "budgeted_volume": item.budgeted_volume, "actual_volume": item.actual_volume,
            "revised_budget_volume": round(revised_budget, 2)
        })
    
    return VolumeResult(
        company_id=req.company_id, period=req.period,
        total_volume_variance=round(total_vol_var, 2),
        total_mix_variance=round(total_mix, 2),
        total_quantity_variance=round(total_qty, 2),
        items=items_result
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
