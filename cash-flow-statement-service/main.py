"""Cash Flow Statement Service - Port 8335"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Cash Flow Statement Service", version="1.0.0")

class CashFlowRequest(BaseModel):
    company_id: str; operating_cash: float; investing_cash: float; financing_cash: float; opening_cash: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "cash-flow-statement"}

@app.post("/prepare", response_model=dict)
async def prepare_cash_flow(request: CashFlowRequest):
    net_cash = request.operating_cash + request.investing_cash + request.financing_cash
    closing_cash = request.opening_cash + net_cash
    return {"company_id": request.company_id, "net_cash_flow": net_cash, "closing_cash": closing_cash, "free_cash_flow": request.operating_cash + request.investing_cash}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8335)
