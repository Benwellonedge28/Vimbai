"""Treasury Compliance Service - Port 8325"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Treasury Compliance Service", version="1.0.0")

class TreasuryComplianceRequest(BaseModel):
    company_id: str; transactions: list

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-compliance"}

@app.post("/check", response_model=Dict[str, Any])
async def check_compliance(request: TreasuryComplianceRequest):
    return {"company_id": request.company_id, "transactions_checked": len(request.transactions), "compliant": True}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8325)
