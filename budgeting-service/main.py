"""Budgeting Service - Port 8302"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Budgeting Service", version="1.0.0")

class BudgetingRequest(BaseModel):
    company_id: str; revenue: float; cogs: float; opex: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "budgeting"}

@app.post("/prepare", response_model=Dict[str, Any])
async def prepare_budget(request: BudgetingRequest):
    gross_profit = request.revenue - request.cogs
    ebitda = gross_profit - request.opex
    return {"company_id": request.company_id, "revenue": request.revenue, "gross_profit": gross_profit, "ebitda": ebitda}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8302)
