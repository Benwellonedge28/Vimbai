"""
Vimbai Throughput Accounting Service
Theory of Constraints-based throughput accounting and product mix optimization.
Port: 8383
"""
import os, uuid
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "throughput-accounting-service"
PORT = int(os.getenv("PORT", "8383"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Throughput Accounting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class Product(BaseModel):
    name: str; selling_price: float; material_cost: float
    time_on_constraint: float  # minutes on bottleneck resource
    demand: int = 0

class ThroughputRequest(BaseModel):
    company_id: str; operating_expenses: float
    products: List[Product]; available_constraint_minutes: int = 480

class ThroughputResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    total_throughput: float; operating_expenses: float; net_profit: float
    roi: float; product_ranking: List[Dict]
    optimal_mix: List[Dict]
    constraint_utilization: float

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/analyze", response_model=ThroughputResult)
async def analyze_throughput(req: ThroughputRequest):
    products = []
    for p in req.products:
        throughput_per_unit = p.selling_price - p.material_cost
        throughput_per_minute = throughput_per_unit / p.time_on_constraint if p.time_on_constraint > 0 else 0
        products.append({
            "name": p.name, "throughput_per_unit": round(throughput_per_unit, 2),
            "throughput_per_minute": round(throughput_per_minute, 2),
            "demand": p.demand, "time_on_constraint": p.time_on_constraint,
            "total_throughput_possible": round(throughput_per_unit * p.demand, 2)
        })
    
    products.sort(key=lambda x: x["throughput_per_minute"], reverse=True)
    
    remaining_minutes = req.available_constraint_minutes
    optimal_mix = []
    total_throughput = 0
    
    for p in products:
        minutes_needed = p["demand"] * p["time_on_constraint"]
        if remaining_minutes >= minutes_needed:
            produce = p["demand"]
            remaining_minutes -= minutes_needed
        else:
            produce = int(remaining_minutes / p["time_on_constraint"]) if p["time_on_constraint"] > 0 else 0
            remaining_minutes = 0
        tpu = p["throughput_per_unit"]
        contribution = tpu * produce
        total_throughput += contribution
        optimal_mix.append({
            "product": p["name"], "produce": produce,
            "demand": p["demand"], "throughput_contribution": round(contribution, 2)
        })
    
    net_profit = total_throughput - req.operating_expenses
    roi = (net_profit / req.operating_expenses * 100) if req.operating_expenses else 0
    utilization = (req.available_constraint_minutes - remaining_minutes) / req.available_constraint_minutes * 100 if req.available_constraint_minutes else 0
    
    return ThroughputResult(
        company_id=req.company_id,
        total_throughput=round(total_throughput, 2),
        operating_expenses=round(req.operating_expenses, 2),
        net_profit=round(net_profit, 2),
        roi=round(roi, 2),
        product_ranking=[{"name": p["name"], "tpm": p["throughput_per_minute"]} for p in products],
        optimal_mix=optimal_mix,
        constraint_utilization=round(utilization, 1)
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
