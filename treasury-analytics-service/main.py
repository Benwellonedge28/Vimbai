"""Treasury Analytics Service - Port 8326"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Treasury Analytics Service", version="1.0.0")

class TreasuryAnalyticsRequest(BaseModel):
    company_id: str; metrics: dict

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-analytics"}

@app.post("/analyze", response_model=Dict[str, Any])
async def analyze_treasury(request: TreasuryAnalyticsRequest):
    return {"company_id": request.company_id, "metrics": request.metrics, "status": "Analyzed"}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8326)
