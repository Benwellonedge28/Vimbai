"""Expense Tracking Service - Port 8338"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Expense Tracking Service", version="1.0.0")

class ExpenseRequest(BaseModel):
    company_id: str; expenses: list; budget: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "expense-tracking"}

@app.post("/track", response_model=dict)
async def track_expenses(request: ExpenseRequest):
    total_expenses = sum(e.get("amount", 0) for e in request.expenses)
    return {"company_id": request.company_id, "total_expenses": total_expenses, "budget": request.budget, "variance": round(request.budget - total_expenses, 2), "utilization_pct": round(total_expenses / request.budget * 100, 2) if request.budget else 0}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8338)
