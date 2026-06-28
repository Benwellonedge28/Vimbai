"""Product Costing Service - Port 8340"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Product Costing Service", version="1.0.0")

class ProductCostingRequest(BaseModel):
    company_id: str; products: list; allocation_base: str

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "product-costing"}

@app.post("/cost", response_model=dict)
async def cost_products(request: ProductCostingRequest):
    return {"company_id": request.company_id, "products_analyzed": len(request.products), "allocation_method": request.allocation_base}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8340)
