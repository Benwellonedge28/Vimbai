"""Corporate Tax Service - Port 8298"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Corporate Tax Service", version="1.0.0")

class CorporateTaxRequest(BaseModel):
    company_id: str; taxable_income: float; tax_rate: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "corporate-tax"}

@app.post("/calculate", response_model=Dict[str, Any])
async def calculate_tax(request: CorporateTaxRequest):
    tax = request.taxable_income * request.tax_rate
    return {"company_id": request.company_id, "taxable_income": request.taxable_income, "tax_rate": request.tax_rate, "tax_liability": tax, "effective_rate": request.tax_rate}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8298)
