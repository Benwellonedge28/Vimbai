"""
Vimbai Foreign Currency Translation Service
IAS 21 functional currency determination and foreign operation translation.
Port: 8388
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "foreign-currency-translation-service"
PORT = int(os.getenv("PORT", "8388"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Foreign Currency Translation Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class BalanceSheetItem(BaseModel):
    account: str
    amount: float
    is_monetary: bool = True


class TranslationRequest(BaseModel):
    company_id: str
    subsidiary: str
    functional_currency: str
    presentation_currency: str
    closing_rate: float
    avg_rate: float
    historical_rate: float
    net_assets: List[BalanceSheetItem] = []
    income_statement: List[BalanceSheetItem] = []
    goodwill: float = 0


class TranslationResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    subsidiary: str
    functional_currency: str
    presentation_currency: str
    translated_net_assets: float
    translated_income: float
    cumulative_translation_adjustment: float
    goodwill_translated: float
    exchange_differences: float
    details: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/translate", response_model=TranslationResult)
async def translate_financials(req: TranslationRequest):
    translated_assets = 0
    translated_income = 0
    details = []

    for item in req.net_assets:
        rate = req.closing_rate if item.is_monetary else req.historical_rate
        translated = item.amount * rate
        translated_assets += translated
        details.append(
            {
                "account": item.account,
                "original": round(item.amount, 2),
                "rate_used": rate,
                "rate_type": "closing" if item.is_monetary else "historical",
                "translated": round(translated, 2),
            }
        )

    for item in req.income_statement:
        translated = item.amount * req.avg_rate
        translated_income += translated
        details.append(
            {
                "account": item.account,
                "original": round(item.amount, 2),
                "rate_used": req.avg_rate,
                "rate_type": "average",
                "translated": round(translated, 2),
            }
        )

    goodwill_translated = req.goodwill * req.closing_rate
    exchange_diff = (
        sum(i.amount for i in req.net_assets if i.is_monetary) * req.closing_rate
        - sum(i.amount for i in req.net_assets if i.is_monetary) * req.avg_rate
    )
    cta = exchange_diff

    return TranslationResult(
        company_id=req.company_id,
        subsidiary=req.subsidiary,
        functional_currency=req.functional_currency,
        presentation_currency=req.presentation_currency,
        translated_net_assets=round(translated_assets, 2),
        translated_income=round(translated_income, 2),
        cumulative_translation_adjustment=round(cta, 2),
        goodwill_translated=round(goodwill_translated, 2),
        exchange_differences=round(exchange_diff, 2),
        details=details,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
