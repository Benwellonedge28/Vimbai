"""Income Statement Service - Port 8334"""
import httpx; import structlog; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Income Statement Service", version="1.0.0")

class IncomeStatementRequest(BaseModel):
    company_id: str; revenue: float; cogs: float; opex: float; interest_expense: float; tax_rate: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "income-statement"}

@app.post("/prepare", response_model=dict)
async def prepare_income_statement(request: IncomeStatementRequest):
    gross_profit = request.revenue - request.cogs
    ebit = gross_profit - request.opex
    ebt = ebit - request.interest_expense
    tax = ebt * request.tax_rate
    net_income = ebt - tax
    return {"company_id": request.company_id, "revenue": request.revenue, "gross_profit": gross_profit, "ebit": ebit, "net_income": net_income, "profit_margin": round(net_income / request.revenue * 100, 2) if request.revenue else 0}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8334)
