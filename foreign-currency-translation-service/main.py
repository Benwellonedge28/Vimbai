"""
Foreign Currency Translation Service
Port: 8373
FX translation for foreign operations
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Foreign Currency Translation Service", version="1.0.0")

class TranslationRequest(BaseModel):
    company_id: str
    entity_currency: str
    functional_currency: str
    balances: Dict[str, float]
    exchange_rates: Dict[str, float]
    translation_date: date

class TranslationResponse(BaseModel):
    company_id: str
    translated_balances: Dict[str, float]
    translation_adjustment: float
    cumulative_adjustment: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "foreign-currency-translation", "version": "1.0.0"}

@app.post("/translate", response_model=TranslationResponse)
async def translate_balances(request: TranslationRequest):
    logger.info("Translating foreign currency", company=request.company_id)
    
    translated = {}
    translation_adj = 0.0
    rate = request.exchange_rates.get(request.entity_currency, 1.0)
    
    for acct, balance in request.balances.items():
        translated[acct] = round(balance * rate, 2)
        if "equity" in acct.lower():
            translation_adj += balance * (rate - 1)
    
    return TranslationResponse(
        company_id=request.company_id,
        translated_balances=translated,
        translation_adjustment=round(translation_adj, 2),
        cumulative_adjustment=round(translation_adj * 1.5, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8373)
