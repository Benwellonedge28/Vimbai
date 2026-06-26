"""Tax Audit Service - Port 8300"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Tax Audit Service", version="1.0.0")

class TaxAuditRequest(BaseModel):
    company_id: str; tax_period: str; assessment: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "tax-audit"}

@app.post("/respond", response_model=Dict[str, Any])
async def respond_to_audit(request: TaxAuditRequest):
    return {"company_id": request.company_id, "period": request.tax_period, "assessment": request.assessment, "status": "Response Filed"}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8300)
