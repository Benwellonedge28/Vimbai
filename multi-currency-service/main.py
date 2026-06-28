"""
Multi-Currency Service
Port: 8349
Foreign exchange and multi-currency transaction handling
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Multi-Currency Service", version="1.0.0")

class FXRate(BaseModel):
    from_currency: str
    to_currency: str
    rate: float
    bid: float
    ask: float
    timestamp: datetime

class CurrencyConversionRequest(BaseModel):
    company_id: str
    amount: float
    from_currency: str
    to_currency: str
    conversion_date: Optional[date] = None

class CurrencyConversionResponse(BaseModel):
    company_id: str
    original_amount: float
    original_currency: str
    converted_amount: float
    target_currency: str
    exchange_rate: float
    spread: float
    converted_at: datetime

class TransactionRequest(BaseModel):
    company_id: str
    transaction_id: str
    amount: float
    currency: str
    transaction_type: str
    counterparty_currency: Optional[str] = None

class TransactionResponse(BaseModel):
    transaction_id: str
    amount_in_functional: float
    functional_currency: str
    fx_gain_loss: float
    exchange_rate_used: float
    booking_status: str

class HedgeRequest(BaseModel):
    company_id: str
    exposure_amount: float
    currency_pair: str
    hedge_type: str
    tenor: int
    notional: float

class HedgeResponse(BaseModel):
    hedge_id: str
    instrument_type: str
    strike_rate: float
    premium: float
    effectiveness_ratio: float
    mark_to_market: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "multi-currency", "version": "1.0.0"}

@app.post("/convert", response_model=CurrencyConversionResponse)
async def convert_currency(request: CurrencyConversionRequest):
    logger.info("Converting currency", company=request.company_id, amount=request.amount, from_curr=request.from_currency, to_curr=request.to_currency)
    
    rates = {"EURUSD": 1.10, "GBPEUR": 1.17, "USDJPY": 149.5, "USDEUR": 0.91}
    pair = f"{request.from_currency}{request.to_currency}"
    reverse_pair = f"{request.to_currency}{request.from_currency}"
    
    if pair in rates:
        rate = rates[pair]
    elif reverse_pair in rates:
        rate = 1 / rates[reverse_pair]
    else:
        rate = 1.0
    
    spread = rate * 0.002
    converted = request.amount * (rate - spread)
    
    return CurrencyConversionResponse(
        company_id=request.company_id,
        original_amount=request.amount,
        original_currency=request.from_currency,
        converted_amount=round(converted, 2),
        target_currency=request.to_currency,
        exchange_rate=round(rate, 6),
        spread=round(spread, 6),
        converted_at=datetime.now()
    )

@app.post("/transaction", response_model=TransactionResponse)
async def process_transaction(request: TransactionRequest):
    logger.info("Processing multi-currency transaction", company=request.company_id, txn_id=request.transaction_id)
    
    fx_gain_loss = 0.0
    if request.counterparty_currency and request.counterparty_currency != "USD":
        fx_gain_loss = request.amount * 0.01
    
    return TransactionResponse(
        transaction_id=request.transaction_id,
        amount_in_functional=round(request.amount * 1.0, 2),
        functional_currency="USD",
        fx_gain_loss=round(fx_gain_loss, 2),
        exchange_rate_used=1.0,
        booking_status="posted"
    )

@app.post("/hedge", response_model=HedgeResponse)
async def create_hedge(request: HedgeRequest):
    logger.info("Creating hedge", company=request.company_id, type=request.hedge_type)
    
    return HedgeResponse(
        hedge_id=f"HEDGE-{datetime.now().strftime('%Y%m%d%H%M')}",
        instrument_type=request.hedge_type,
        strike_rate=1.10,
        premium=round(request.notional * 0.02, 2),
        effectiveness_ratio=0.95,
        mark_to_market=round(request.exposure_amount * 0.015, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8349)
