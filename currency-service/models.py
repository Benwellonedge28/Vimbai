"""Pydantic models for Currency Service (API contract unchanged)."""

from datetime import datetime
from typing import Any, Dict, List, Optional

from pydantic import BaseModel, Field


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
# Default Currencies

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


class MultiCurrencyTransaction(BaseModel):
    base_currency: str
    lines: List[Dict[str, Any]]  # currency, amount, account, description
