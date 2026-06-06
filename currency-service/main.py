"""
FinAcc Multi-Currency Service
Provides comprehensive currency management, conversion, and exchange rate handling
"""

from fastapi import FastAPI, HTTPException, Depends, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
from datetime import datetime, timedelta
from decimal import Decimal, ROUND_HALF_UP
import asyncio
import os
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Currency Service",
    description="Multi-currency support for financial transactions",
    version="0.1.0",
)

# ============================================================================
# Models
# ============================================================================

class Currency(BaseModel):
    code: str = Field(..., min_length=3, max_length=3)  # ISO 4217 code
    name: str
    symbol: str
    decimal_places: int = Field(default=2, ge=0, le=4)
    is_active: bool = True

class ExchangeRate(BaseModel):
    from_currency: str
    to_currency: str
    rate: float = Field(..., gt=0)
    effective_date: datetime
    source: str = "manual"  # manual, api, ecb, etc.
    last_updated: datetime = Field(default_factory=datetime.utcnow)

class ConversionRequest(BaseModel):
    from_currency: str
    to_currency: str
    amount: float = Field(..., gt=0)
    rate_date: Optional[datetime] = None  # None means current rate

class ConversionResult(BaseModel):
    from_currency: str
    to_currency: str
    original_amount: float
    converted_amount: float
    rate_used: float
    rate_date: datetime
    rounding_mode: str = "HALF_UP"

class ExchangeRateCreate(BaseModel):
    from_currency: str
    to_currency: str
    rate: float = Field(..., gt=0)
    effective_date: Optional[datetime] = None
    source: str = "manual"

class CurrencyCreate(BaseModel):
    code: str = Field(..., min_length=3, max_length=3)
    name: str
    symbol: str
    decimal_places: int = Field(default=2, ge=0, le=4)

class ExchangeRateUpdate(BaseModel):
    rate: float = Field(..., gt=0)
    source: Optional[str] = None

# ============================================================================
# Default Currencies and Exchange Rates
# ============================================================================

DEFAULT_CURRENCIES = {
    "USD": {"name": "US Dollar", "symbol": "$", "decimal_places": 2},
    "EUR": {"name": "Euro", "symbol": "€", "decimal_places": 2},
    "GBP": {"name": "British Pound", "symbol": "£", "decimal_places": 2},
    "JPY": {"name": "Japanese Yen", "symbol": "¥", "decimal_places": 0},
    "CNY": {"name": "Chinese Yuan", "symbol": "¥", "decimal_places": 2},
    "INR": {"name": "Indian Rupee", "symbol": "₹", "decimal_places": 2},
    "CAD": {"name": "Canadian Dollar", "symbol": "C$", "decimal_places": 2},
    "AUD": {"name": "Australian Dollar", "symbol": "A$", "decimal_places": 2},
    "CHF": {"name": "Swiss Franc", "symbol": "CHF", "decimal_places": 2},
    "HKD": {"name": "Hong Kong Dollar", "symbol": "HK$", "decimal_places": 2},
    "SGD": {"name": "Singapore Dollar", "symbol": "S$", "decimal_places": 2},
    "SEK": {"name": "Swedish Krona", "symbol": "kr", "decimal_places": 2},
    "NOK": {"name": "Norwegian Krone", "symbol": "kr", "decimal_places": 2},
    "MXN": {"name": "Mexican Peso", "symbol": "$", "decimal_places": 2},
    "BRL": {"name": "Brazilian Real", "symbol": "R$", "decimal_places": 2},
}

# Default exchange rates (relative to USD)
DEFAULT_RATES = {
    ("USD", "USD"): 1.0,
    ("USD", "EUR"): 0.92,
    ("USD", "GBP"): 0.79,
    ("USD", "JPY"): 149.50,
    ("USD", "CNY"): 7.24,
    ("USD", "INR"): 83.12,
    ("USD", "CAD"): 1.36,
    ("USD", "AUD"): 1.53,
    ("USD", "CHF"): 0.88,
    ("USD", "HKD"): 7.82,
    ("USD", "SGD"): 1.34,
    ("USD", "SEK"): 10.42,
    ("USD", "NOK"): 10.65,
    ("USD", "MXN"): 17.15,
    ("USD", "BRL"): 4.97,
}

# In-memory storage (use database in production)
currencies: Dict[str, Currency] = {}
exchange_rates: List[ExchangeRate] = []

# Initialize default currencies
for code, data in DEFAULT_CURRENCIES.items():
    currencies[code] = Currency(
        code=code,
        name=data["name"],
        symbol=data["symbol"],
        decimal_places=data["decimal_places"]
    )

# Initialize default rates
for (from_curr, to_curr), rate in DEFAULT_RATES.items():
    exchange_rates.append(ExchangeRate(
        from_currency=from_curr,
        to_currency=to_curr,
        rate=rate,
        effective_date=datetime.utcnow(),
        source="default"
    ))

# ============================================================================
# Currency Conversion Engine
# ============================================================================

class CurrencyConverter:
    """Handles currency conversions with precision"""

    def __init__(self, rates: List[ExchangeRate]):
        self.rates = rates
        self._build_rate_map()

    def _build_rate_map(self):
        """Build lookup map for exchange rates"""
        self.rate_map = {}
        for rate in self.rates:
            key = (rate.from_currency, rate.to_currency)
            self.rate_map[key] = rate

    def get_rate(self, from_curr: str, to_curr: str, date: Optional[datetime] = None) -> Optional[float]:
        """Get exchange rate between two currencies"""
        # Direct rate
        key = (from_curr, to_curr)
        if key in self.rate_map:
            return self.rate_map[key].rate

        # Inverse rate
        inverse_key = (to_curr, from_curr)
        if inverse_key in self.rate_map:
            return 1 / self.rate_map[inverse_key].rate

        # Cross rate through USD
        if from_curr != "USD" and to_curr != "USD":
            from_usd = self.get_rate("USD", from_curr)
            usd_to = self.get_rate("USD", to_curr)
            if from_usd and usd_to:
                return usd_to / from_usd

        return None

    def convert(
        self,
        from_curr: str,
        to_curr: str,
        amount: float,
        date: Optional[datetime] = None,
        rounding: str = "HALF_UP"
    ) -> ConversionResult:
        """Convert amount from one currency to another"""
        rate = self.get_rate(from_curr, to_curr, date)
        if rate is None:
            raise ValueError(f"No exchange rate found for {from_curr} to {to_curr}")

        # Perform conversion
        converted = amount * rate

        # Apply rounding
        decimal_places = currencies.get(to_curr, Currency(code=to_curr, name="", symbol="")).decimal_places
        rounding_mode = ROUND_HALF_UP if rounding == "HALF_UP" else ROUND_HALF_UP

        converted = float(Decimal(str(converted)).quantize(
            Decimal(10) ** -decimal_places,
            rounding=rounding_mode
        ))

        return ConversionResult(
            from_currency=from_curr,
            to_currency=to_curr,
            original_amount=amount,
            converted_amount=converted,
            rate_used=rate,
            rate_date=date or datetime.utcnow(),
            rounding_mode=rounding
        )

converter = CurrencyConverter(exchange_rates)

# ============================================================================
# API Endpoints
# ============================================================================

@app.on_event("startup")
async def startup():
    """Initialize service"""
    print("Currency service started")

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "currency"}

# --- Currency Management ---
@app.get("/currencies", response_model=List[Currency])
async def list_currencies(active_only: bool = False):
    """List all supported currencies"""
    result = list(currencies.values())
    if active_only:
        result = [c for c in result if c.is_active]
    return result

@app.get("/currencies/{code}", response_model=Currency)
async def get_currency(code: str):
    """Get a specific currency"""
    code = code.upper()
    if code not in currencies:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Currency not found")
    return currencies[code]

@app.post("/currencies", response_model=Currency, status_code=status.HTTP_201_CREATED)
async def create_currency(currency: CurrencyCreate):
    """Add a new currency"""
    code = currency.code.upper()
    if code in currencies:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Currency already exists")

    new_currency = Currency(
        code=code,
        name=currency.name,
        symbol=currency.symbol,
        decimal_places=currency.decimal_places
    )
    currencies[code] = new_currency
    return new_currency

@app.put("/currencies/{code}", response_model=Currency)
async def update_currency(code: str, currency: CurrencyCreate):
    """Update a currency"""
    code = code.upper()
    if code not in currencies:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Currency not found")

    currencies[code].name = currency.name
    currencies[code].symbol = currency.symbol
    currencies[code].decimal_places = currency.decimal_places
    return currencies[code]

@app.delete("/currencies/{code}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_currency(code: str):
    """Deactivate a currency (soft delete)"""
    code = code.upper()
    if code not in currencies:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Currency not found")

    currencies[code].is_active = False

# --- Exchange Rate Management ---
@app.get("/rates", response_model=List[ExchangeRate])
async def list_exchange_rates(
    from_currency: Optional[str] = None,
    to_currency: Optional[str] = None
):
    """List exchange rates with optional filters"""
    result = exchange_rates
    if from_currency:
        result = [r for r in result if r.from_currency == from_currency.upper()]
    if to_currency:
        result = [r for r in result if r.to_currency == to_currency.upper()]

    # Sort by effective date (most recent first)
    result.sort(key=lambda x: x.effective_date, reverse=True)

    return result

@app.get("/rates/latest", response_model=List[ExchangeRate])
async def get_latest_rates():
    """Get the most recent exchange rate for each currency pair"""
    latest = {}
    for rate in exchange_rates:
        key = (rate.from_currency, rate.to_currency)
        if key not in latest or rate.effective_date > latest[key].effective_date:
            latest[key] = rate
    return list(latest.values())

@app.get("/rates/{from_currency}/{to_currency}", response_model=ExchangeRate)
async def get_exchange_rate(from_currency: str, to_currency: str):
    """Get exchange rate between two currencies"""
    from_currency = from_currency.upper()
    to_currency = to_currency.upper()

    # Find the most recent rate
    rates = [r for r in exchange_rates
             if r.from_currency == from_currency and r.to_currency == to_currency]

    if not rates:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND,
                          detail=f"No rate found for {from_currency} to {to_currency}")

    return max(rates, key=lambda x: x.effective_date)

@app.post("/rates", response_model=ExchangeRate, status_code=status.HTTP_201_CREATED)
async def create_exchange_rate(rate: ExchangeRateCreate):
    """Add a new exchange rate"""
    new_rate = ExchangeRate(
        from_currency=rate.from_currency.upper(),
        to_currency=rate.to_currency.upper(),
        rate=rate.rate,
        effective_date=rate.effective_date or datetime.utcnow(),
        source=rate.source
    )
    exchange_rates.append(new_rate)

    # Update converter's rate map
    converter.rates = exchange_rates
    converter._build_rate_map()

    return new_rate

@app.put("/rates/{from_currency}/{to_currency}", response_model=ExchangeRate)
async def update_exchange_rate(
    from_currency: str,
    to_currency: str,
    update: ExchangeRateUpdate
):
    """Update an exchange rate (creates new rate entry)"""
    from_currency = from_currency.upper()
    to_currency = to_currency.upper()

    new_rate = ExchangeRate(
        from_currency=from_currency,
        to_currency=to_currency,
        rate=update.rate,
        effective_date=datetime.utcnow(),
        source=update.source or "manual"
    )
    exchange_rates.append(new_rate)

    # Update converter
    converter.rates = exchange_rates
    converter._build_rate_map()

    return new_rate

# --- Currency Conversion ---
@app.post("/convert", response_model=ConversionResult)
async def convert_currency(request: ConversionRequest):
    """Convert an amount from one currency to another"""
    try:
        return converter.convert(
            from_curr=request.from_currency.upper(),
            to_curr=request.to_currency.upper(),
            amount=request.amount,
            date=request.rate_date
        )
    except ValueError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))

@app.post("/convert/batch")
async def convert_batch(requests: List[ConversionRequest]):
    """Convert multiple amounts at once"""
    results = []
    errors = []

    for i, req in enumerate(requests):
        try:
            result = converter.convert(
                from_curr=req.from_currency.upper(),
                to_curr=req.to_currency.upper(),
                amount=req.amount,
                date=req.rate_date
            )
            results.append({"index": i, "success": True, "result": result})
        except ValueError as e:
            errors.append({"index": i, "success": False, "error": str(e)})

    return {
        "total": len(requests),
        "successful": len(results),
        "failed": len(errors),
        "results": results,
        "errors": errors
    }

# --- Triangulation ---
@app.post("/triangulate")
async def triangulate(
    from_currency: str,
    to_currency: str,
    amount: float
):
    """Compare direct vs cross-rate conversion"""
    from_currency = from_currency.upper()
    to_currency = to_currency.upper()

    # Direct conversion
    direct = converter.convert(from_currency, to_currency, amount)

    # Cross through USD
    if from_currency != "USD" and to_currency != "USD":
        mid = converter.convert(from_currency, "USD", amount)
        cross = converter.convert("USD", to_currency, mid.converted_amount)

        difference = abs(direct.converted_amount - cross.converted_amount)
        savings = (1 - cross.converted_amount / direct.converted_amount) * 100 if direct.converted_amount > 0 else 0

        return {
            "direct_conversion": direct,
            "cross_conversion": cross,
            "difference": difference,
            "potential_savings_percent": savings,
            "recommendation": "Use cross-rate" if savings > 0.1 else "Use direct rate"
        }
    else:
        return {
            "direct_conversion": direct,
            "cross_conversion": None,
            "recommendation": "No triangulation needed (same base currency)"
        }

# --- Historical Rates ---
@app.get("/rates/history/{from_currency}/{to_currency}")
async def get_historical_rates(
    from_currency: str,
    to_currency: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100
):
    """Get historical exchange rates"""
    from_currency = from_currency.upper()
    to_currency = to_currency.upper()

    rates = [r for r in exchange_rates
             if r.from_currency == from_currency and r.to_currency == to_currency]

    if start_date:
        rates = [r for r in rates if r.effective_date >= start_date]
    if end_date:
        rates = [r for r in rates if r.effective_date <= end_date]

    # Sort by date
    rates.sort(key=lambda x: x.effective_date, reverse=True)

    return {
        "from_currency": from_currency,
        "to_currency": to_currency,
        "count": len(rates[:limit]),
        "rates": rates[:limit]
    }

# --- Currency Formatting ---
@app.get("/format/{currency_code}/{amount}")
async def format_amount(currency_code: str, amount: float):
    """Format amount with currency symbol"""
    currency_code = currency_code.upper()

    if currency_code not in currencies:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Currency not found")

    currency = currencies[currency_code]
    decimal_places = currency.decimal_places

    formatted = f"{currency.symbol}{amount:,.{decimal_places}f}"

    return {
        "currency": currency_code,
        "symbol": currency.symbol,
        "amount": amount,
        "formatted": formatted,
        "decimal_places": decimal_places
    }

# --- Multi-Currency Transaction Support ---
class MultiCurrencyTransaction(BaseModel):
    base_currency: str
    lines: List[Dict[str, Any]]  # currency, amount, account, description

@app.post("/validate-transaction")
async def validate_multicurrency_transaction(transaction: MultiCurrencyTransaction):
    """Validate a multi-currency transaction"""
    base_currency = transaction.base_currency.upper()

    if base_currency not in currencies:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST,
                          detail=f"Base currency {base_currency} not supported")

    # Convert all amounts to base currency
    converted_lines = []
    total_base = Decimal("0")

    for line in transaction.lines:
        currency = line.get("currency", base_currency).upper()
        amount = Decimal(str(line.get("amount", 0)))

        if currency != base_currency:
            result = converter.convert(base_currency, currency, float(amount))
            converted_amount = result.converted_amount
        else:
            converted_amount = float(amount)

        converted_lines.append({
            "original_currency": currency,
            "original_amount": float(amount),
            "converted_amount": converted_amount,
            "converted_to": base_currency,
            "account": line.get("account"),
            "description": line.get("description")
        })

        total_base += Decimal(str(converted_amount))

    return {
        "valid": True,
        "base_currency": base_currency,
        "total_in_base": float(total_base),
        "line_count": len(transaction.lines),
        "lines": converted_lines
    }

# --- Rate Alerts ---
@app.post("/alerts/rate")
async def create_rate_alert(
    from_currency: str,
    to_currency: str,
    target_rate: float,
    direction: Literal["above", "below", "any"],
    notification_url: Optional[str] = None
):
    """Create an alert when exchange rate reaches target"""
    return {
        "alert_id": f"rate_alert_{from_currency}_{to_currency}",
        "from_currency": from_currency.upper(),
        "to_currency": to_currency.upper(),
        "target_rate": target_rate,
        "direction": direction,
        "notification_url": notification_url,
        "created_at": datetime.utcnow().isoformat(),
        "status": "active"
    }

# --- Statistics ---
@app.get("/stats")
async def get_currency_stats():
    """Get currency service statistics"""
    return {
        "total_currencies": len(currencies),
        "active_currencies": sum(1 for c in currencies.values() if c.is_active),
        "total_rates": len(exchange_rates),
        "latest_rate_date": max(r.effective_date for r in exchange_rates).isoformat()
        if exchange_rates else None
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8092)