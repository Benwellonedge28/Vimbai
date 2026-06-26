"""Sensitivity Analysis Service - Port 8308"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Sensitivity Analysis Service", version="1.0.0")

class SensitivityRequest(BaseModel):
    company_id: str; variables: list

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "sensitivity-analysis"}

@app.post("/analyze", response_model=Dict[str, Any])
async def analyze_sensitivity(request: SensitivityRequest):
    return {"company_id": request.company_id, "variables_analyzed": len(request.variables), "sensitivity_ratios": {}}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8308)
