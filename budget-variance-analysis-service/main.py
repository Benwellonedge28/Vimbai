"""Budget Variance Analysis Service - Port 8305"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Budget Variance Analysis Service", version="1.0.0")

class VarianceAnalysisRequest(BaseModel):
    company_id: str; items: list

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "budget-variance-analysis"}

@app.post("/analyze", response_model=Dict[str, Any])
async def analyze_variance(request: VarianceAnalysisRequest):
    return {"company_id": request.company_id, "items_analyzed": len(request.items), "significant_variances": 0}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8305)
