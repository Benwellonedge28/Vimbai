"""Balance Sheet Service - Port 8333"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Balance Sheet Service", version="1.0.0")

class BalanceSheetRequest(BaseModel):
    company_id: str; current_assets: float; non_current_assets: float; current_liabilities: float; non_current_liabilities: float; equity: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "balance-sheet"}

@app.post("/prepare", response_model=dict)
async def prepare_balance_sheet(request: BalanceSheetRequest):
    total_assets = request.current_assets + request.non_current_assets
    total_liabilities = request.current_liabilities + request.non_current_liabilities
    return {"company_id": request.company_id, "total_assets": total_assets, "total_liabilities": total_liabilities, "equity": request.equity, "balanced": abs(total_assets - total_liabilities - request.equity) < 0.01}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8333)
