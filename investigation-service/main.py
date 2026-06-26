"""Investigation Service - Port 8314"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Investigation Service", version="1.0.0")

class InvestigationRequest(BaseModel):
    company_id: str; scope: str

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "investigation"}

@app.post("/start", response_model=Dict[str, Any])
async def start_investigation(request: InvestigationRequest):
    return {"company_id": request.company_id, "scope": request.scope, "status": "Active"}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8314)
