"""Revenue Recognition Service - Port 8337"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Revenue Recognition Service", version="1.0.0")

class RevenueRequest(BaseModel):
    company_id: str; total_contract_value: float; performance_obligation_met: float; total_obligation: float; recognition_method: str

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "revenue-recognition"}

@app.post("/recognize", response_model=dict)
async def recognize_revenue(request: RevenueRequest):
    completion_pct = request.performance_obligation_met / request.total_obligation if request.total_obligation else 0
    recognized_revenue = request.total_contract_value * completion_pct
    return {"company_id": request.company_id, "completion_percentage": round(completion_pct * 100, 2), "recognized_revenue": round(recognized_revenue, 2), "deferred_revenue": round(request.total_contract_value - recognized_revenue, 2)}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8337)
