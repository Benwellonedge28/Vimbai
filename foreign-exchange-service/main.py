"""
Vimbai Foreign Exchange Service
FX rate management, currency conversion, and hedging calculations.
Port: 8368
"""
import os, uuid, math
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "foreign-exchange-service"
PORT = int(os.getenv("PORT", "8368"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Foreign Exchange Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

# Mock exchange rates (base: USD)
RATES = {
    "USD": 1.0, "EUR": 0.92, "GBP": 0.79, "ZWL": 15.5, "ZAR": 18.5,
    "BWP": 13.6, "KES": 152.0, "NGN": 1600.0, "GHS": 15.2, "CNY": 7.25, "JPY": 149.5
}

class ConversionRequest(BaseModel):
    from_currency: str; to_currency: str; amount: float; rate_date: str = ""

class ConversionResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    from_currency: str; to_currency: str; amount: float
    rate: float; converted_amount: float; rate_date: str

class FXExposureRequest(BaseModel):
    company_id: str; currency: str; exposure_amount: float
    hedge_ratio: float = 1.0; forward_rate: float = 0

class FXHedgeResult(BaseModel):
    company_id: str; currency: str
    exposure_amount: float; hedged_amount: float; unhedged_amount: float
    hedge_ratio: float; forward_rate: float
    potential_loss_unhedged: float; hedge_cost_estimate: float

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0", "currencies": list(RATES.keys())}

@app.get("/rates")
async def get_rates():
    return {"base": "USD", "rates": RATES, "updated": datetime.now(timezone.utc).isoformat()}

@app.get("/rates/{currency}")
async def get_rate(currency: str):
    if currency not in RATES:
        from fastapi import HTTPException; raise HTTPException(status_code=404, detail=f"Currency {currency} not found")
    return {"currency": currency, "rate": RATES[currency], "base": "USD"}

@app.post("/convert", response_model=ConversionResult)
async def convert(req: ConversionRequest):
    if req.from_currency not in RATES or req.to_currency not in RATES:
        from fastapi import HTTPException; raise HTTPException(status_code=400, detail="Unsupported currency")
    
    rate = RATES[req.to_currency] / RATES[req.from_currency]
    converted = req.amount * rate
    
    return ConversionResult(
        from_currency=req.from_currency, to_currency=req.to_currency,
        amount=round(req.amount, 2), rate=round(rate, 6),
        converted_amount=round(converted, 2),
        rate_date=req.rate_date or datetime.now(timezone.utc).strftime("%Y-%m-%d")
    )

@app.post("/hedge", response_model=FXHedgeResult)
async def calculate_hedge(req: FXExposureRequest):
    if req.currency not in RATES:
        from fastapi import HTTPException; raise HTTPException(status_code=400, detail="Unsupported currency")
    
    hedged = req.exposure_amount * req.hedge_ratio
    unhedged = req.exposure_amount - hedged
    fwd_rate = req.forward_rate or RATES[req.currency]
    
    potential_loss = unhedged * abs(fwd_rate - RATES[req.currency]) / RATES[req.currency] * 100
    hedge_cost = hedged * 0.002  # 0.2% typical hedge cost
    
    return FXHedgeResult(
        company_id=req.company_id, currency=req.currency,
        exposure_amount=round(req.exposure_amount, 2),
        hedged_amount=round(hedged, 2), unhedged_amount=round(unhedged, 2),
        hedge_ratio=req.hedge_ratio, forward_rate=round(fwd_rate, 6),
        potential_loss_unhedged=round(potential_loss, 2),
        hedge_cost_estimate=round(hedge_cost, 2)
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
