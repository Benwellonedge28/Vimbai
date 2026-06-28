"""Equity Changes Service - Port 8336"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Equity Changes Service", version="1.0.0")

class EquityRequest(BaseModel):
    company_id: str; opening_equity: float; net_income: float; dividends: float; share_issuances: float; share_repurchases: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "equity-changes"}

@app.post("/analyze", response_model=dict)
async def analyze_equity_changes(request: EquityRequest):
    closing_equity = request.opening_equity + request.net_income - request.dividends + request.share_issuances - request.share_repurchases
    return {"company_id": request.company_id, "opening_equity": request.opening_equity, "closing_equity": closing_equity, "change": round(closing_equity - request.opening_equity, 2)}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8336)
