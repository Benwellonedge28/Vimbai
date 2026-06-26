"""Budget Forecasting Service - Port 8303"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Budget Forecasting Service", version="1.0.0")

class BudgetForecastRequest(BaseModel):
    company_id: str; current_budget: float; growth_rate: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "budget-forecasting"}

@app.post("/forecast", response_model=Dict[str, Any])
async def forecast_budget(request: BudgetForecastRequest):
    projected = request.current_budget * (1 + request.growth_rate)
    return {"company_id": request.company_id, "current": request.current_budget, "projected": projected}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8303)
