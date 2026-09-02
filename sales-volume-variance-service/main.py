"""
Vimbai Sales Volume Variance Service
Sales volume and mix variance analysis with contribution margin approach.
Port: 8401
"""
import os, uuid
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "sales-volume-variance-service"
PORT = int(os.getenv("PORT", "8401"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Sales Volume Variance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class ProductVolume(BaseModel):
    product: str; budgeted_volume: int; actual_volume: int
    budgeted_price: float; budgeted_cost: float

class VolumeVarianceRequest(BaseModel):
    company_id: str; period: str; products: List[ProductVolume]
    total_budgeted_volume: int = 0

class VolumeVarianceResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    volume_variance: float; mix_variance: float; yield_variance: float
    total_variance: float
    product_details: List[Dict] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/calculate", response_model=VolumeVarianceResult)
async def calculate_volume_variance(req: VolumeVarianceRequest):
    total_budget = sum(p.budgeted_volume for p in req.products)
    total_actual = sum(p.actual_volume for p in req.products)
    
    budgeted_cm_total = sum((p.budgeted_price - p.budgeted_cost) * p.budgeted_volume for p in req.products)
    actual_cm_at_budget = sum((p.budgeted_price - p.budgeted_cost) * p.actual_volume for p in req.products)
    
    volume_var = (total_actual - total_budget) * (budgeted_cm_total / total_budget) if total_budget else 0
    
    # Mix variance
    mix_var = 0
    for p in req.products:
        budget_cm = p.budgeted_price - p.budgeted_cost
        expected_at_actual = total_actual * (p.budgeted_volume / total_budget) if total_budget else 0
        mix_var += (p.actual_volume - expected_at_actual) * budget_cm
    
    yield_var = volume_var - mix_var
    total = volume_var
    
    details = []
    for p in req.products:
        cm = p.budgeted_price - p.budgeted_cost
        vol_diff = p.actual_volume - p.budgeted_volume
        details.append({
            "product": p.product, "budgeted_volume": p.budgeted_volume,
            "actual_volume": p.actual_volume, "volume_diff": vol_diff,
            "contribution_margin": round(cm, 2),
            "product_variance": round(vol_diff * cm, 2)
        })
    
    return VolumeVarianceResult(
        company_id=req.company_id, period=req.period,
        volume_variance=round(volume_var, 2),
        mix_variance=round(mix_var, 2),
        yield_variance=round(yield_var, 2),
        total_variance=round(total, 2),
        product_details=details
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
