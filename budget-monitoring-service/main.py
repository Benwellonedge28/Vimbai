"""Budget Monitoring Service - Port 8304"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Budget Monitoring Service", version="1.0.0")

class BudgetMonitorRequest(BaseModel):
    company_id: str; budget: float; actual: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "budget-monitoring"}

@app.post("/monitor", response_model=Dict[str, Any])
async def monitor_budget(request: BudgetMonitorRequest):
    variance = request.actual - request.budget
    pct = variance / request.budget * 100 if request.budget else 0
    return {"company_id": request.company_id, "budget": request.budget, "actual": request.actual, "variance": variance, "variance_pct": pct}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8304)
