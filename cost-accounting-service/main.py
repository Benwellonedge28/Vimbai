"""Cost Accounting Service - Port 8339"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Cost Accounting Service", version="1.0.0")

class CostAccountingRequest(BaseModel):
    company_id: str; direct_materials: float; direct_labor: float; manufacturing_overhead: float; units_produced: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "cost-accounting"}

@app.post("/calculate", response_model=dict)
async def calculate_costs(request: CostAccountingRequest):
    total_cost = request.direct_materials + request.direct_labor + request.manufacturing_overhead
    unit_cost = total_cost / request.units_produced if request.units_produced else 0
    return {"company_id": request.company_id, "total_cost": total_cost, "unit_cost": round(unit_cost, 2), "cost_breakdown": {"materials_pct": round(request.direct_materials / total_cost * 100, 2), "labor_pct": round(request.direct_labor / total_cost * 100, 2), "overhead_pct": round(request.manufacturing_overhead / total_cost * 100, 2)}}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8339)
