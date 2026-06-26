"""R&D Tax Service - Port 8301"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="R&D Tax Service", version="1.0.0")

class RandDTaxRequest(BaseModel):
    company_id: str; rd_expenditure: float; tax_credit_rate: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "r-and-d-tax"}

@app.post("/claim", response_model=Dict[str, Any])
async def claim_rd_tax(request: RandDTaxRequest):
    credit = request.rd_expenditure * request.tax_credit_rate
    return {"company_id": request.company_id, "rd_expenditure": request.rd_expenditure, "tax_credit": credit}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8301)
