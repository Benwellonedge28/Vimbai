"""Tax Provision Service - Port 8299"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Tax Provision Service", version="1.0.0")

class TaxProvisionRequest(BaseModel):
    company_id: str; current_tax: float; deferred_tax: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "tax-provision"}

@app.post("/provision", response_model=Dict[str, Any])
async def calculate_provision(request: TaxProvisionRequest):
    total_provision = request.current_tax + request.deferred_tax
    return {"company_id": request.company_id, "current_tax": request.current_tax, "deferred_tax": request.deferred_tax, "total_provision": total_provision}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8299)
