"""
Vimbai Multi-Currency Service
Provides comprehensive currency management, conversion, and exchange rate handling

Currencies and exchange rates persist in Neo4j, stamped with the caller and
the Book context (X-Book-ID verified upstream by the API gateway). The
original default currencies and USD-relative rates are lazily seeded per
user+Book on first access. Conversion, triangulation, validation and
formatting stay pure computations over the caller's visible rates.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime, timedelta
from decimal import ROUND_HALF_UP, Decimal
from typing import Any, Dict, List, Literal, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "currency_service" not in _sys.modules or not hasattr(_sys.modules.get("currency_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("currency_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["currency_service"] = _pkg
    _sys.modules["currency_service"].__path__ = [_HERE]

from currency_service import crud
from currency_service.database import Neo4jConnector
from currency_service.dependencies import book_id_var, get_db_session, get_user_id
from currency_service.exceptions import CurrencyError
from currency_service.models import (
    ConversionRequest,
    ConversionResult,
    Currency,
    CurrencyCreate,
    ExchangeRate,
    ExchangeRateCreate,
    ExchangeRateUpdate,
    MultiCurrencyTransaction,
)
from dotenv import load_dotenv
from fastapi import Depends, FastAPI, HTTPException, Request, status
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

load_dotenv()

app = FastAPI(
    title="Vimbai Currency Service",
    description="Multi-currency support for financial transactions",
    version="1.0.0",
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="currency-service", instrument_app=app)
except ImportError:
    TRACER = None
    import logging

    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")


@app.exception_handler(CurrencyError)
async def _currency_error(request: Request, exc: CurrencyError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


# ============================================================================
# Currency Conversion Engine (pure, unchanged semantics)
# ============================================================================


class CurrencyConverter:
    """Handles currency conversions with precision"""

    def __init__(self, rates: List[ExchangeRate], currencies: Dict[str, Currency]):
        self.rates = rates
        self.currencies = currencies
        self._build_rate_map()

    def _build_rate_map(self):
        """Build lookup map for exchange rates"""
        self.rate_map = {}
        for rate in self.rates:
            key = (rate.from_currency, rate.to_currency)
            self.rate_map[key] = rate

    def get_rate(self, from_curr: str, to_curr: str, date: Optional[datetime] = None) -> Optional[float]:
        """Get exchange rate between two currencies"""
        key = (from_curr, to_curr)
        if key in self.rate_map:
            return self.rate_map[key].rate

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
        self, from_curr: str, to_curr: str, amount: float, date: Optional[datetime] = None, rounding: str = "HALF_UP"
    ) -> ConversionResult:
        """Convert amount from one currency to another"""
        rate = self.get_rate(from_curr, to_curr, date)
        if rate is None:
            raise ValueError(f"No exchange rate found for {from_curr} to {to_curr}")

        converted = amount * rate

        decimal_places = self.currencies.get(to_curr, Currency(code=to_curr, name="", symbol="")).decimal_places
        rounding_mode = ROUND_HALF_UP if rounding == "HALF_UP" else ROUND_HALF_UP

        converted = float(Decimal(str(converted)).quantize(Decimal(10) ** -decimal_places, rounding=rounding_mode))

        return ConversionResult(
            from_currency=from_curr,
            to_currency=to_curr,
            original_amount=amount,
            converted_amount=converted,
            rate_used=rate,
            rate_date=date or datetime.utcnow(),
            rounding_mode=rounding,
        )


async def _build_converter(session: AsyncSession, user_id: str) -> CurrencyConverter:
    """Rebuild the pure converter from the caller's Book-visible records.

    Uses the most recent rate per pair so conversions match the original
    append-order behavior (where the newest entry overwrote older ones).
    """
    rates = await crud.latest_rates(session, user_id)
    currencies = {c.code: c for c in await crud.list_currencies(session, user_id)}
    return CurrencyConverter(rates, currencies)


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "currency"}


# --- Currency Management ---


@app.get("/currencies", response_model=List[Currency])
async def list_currencies(
    active_only: bool = False,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's supported currencies (defaults seeded on first access)"""
    return await crud.list_currencies(db_session, user_id, active_only=active_only)


@app.get("/currencies/{code}", response_model=Currency)
async def get_currency(
    code: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific currency"""
    return await crud.get_currency(db_session, user_id, code.upper())


@app.post("/currencies", response_model=Currency, status_code=status.HTTP_201_CREATED)
async def create_currency(
    currency: CurrencyCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a new currency"""
    return await crud.create_currency(db_session, user_id, currency)


@app.put("/currencies/{code}", response_model=Currency)
async def update_currency(
    code: str,
    currency: CurrencyCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a currency"""
    return await crud.update_currency(db_session, user_id, code.upper(), currency)


@app.delete("/currencies/{code}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_currency(
    code: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Deactivate a currency (soft delete)"""
    await crud.deactivate_currency(db_session, user_id, code.upper())
    return {"ok": True}


# --- Exchange Rate Management ---


@app.get("/rates", response_model=List[ExchangeRate])
async def list_exchange_rates(
    from_currency: Optional[str] = None,
    to_currency: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's exchange rates with optional filters"""
    return await crud.list_rates(db_session, user_id, from_currency=from_currency, to_currency=to_currency)


@app.get("/rates/latest", response_model=List[ExchangeRate])
async def get_latest_rates(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the most recent exchange rate for each currency pair"""
    return await crud.latest_rates(db_session, user_id)


@app.get("/rates/{from_currency}/{to_currency}", response_model=ExchangeRate)
async def get_exchange_rate(
    from_currency: str,
    to_currency: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the most recent exchange rate between two currencies"""
    return await crud.get_rate(db_session, user_id, from_currency, to_currency)


@app.post("/rates", response_model=ExchangeRate, status_code=status.HTTP_201_CREATED)
async def create_exchange_rate(
    rate: ExchangeRateCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a new exchange rate"""
    return await crud.create_rate(db_session, user_id, rate)


@app.put("/rates/{from_currency}/{to_currency}", response_model=ExchangeRate)
async def update_exchange_rate(
    from_currency: str,
    to_currency: str,
    update: ExchangeRateUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update an exchange rate (creates new rate entry, original semantics)"""
    return await crud.update_rate(db_session, user_id, from_currency, to_currency, update.rate, update.source)


# --- Currency Conversion ---


@app.post("/convert", response_model=ConversionResult)
async def convert_currency(
    request: ConversionRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Convert an amount from one currency to another using the caller's rates"""
    converter = await _build_converter(db_session, user_id)
    try:
        return converter.convert(
            from_curr=request.from_currency.upper(),
            to_curr=request.to_currency.upper(),
            amount=request.amount,
            date=request.rate_date,
        )
    except ValueError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))


@app.post("/convert/batch")
async def convert_batch(
    requests: List[ConversionRequest],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Convert multiple amounts at once"""
    converter = await _build_converter(db_session, user_id)
    results = []
    errors = []

    for i, req in enumerate(requests):
        try:
            result = converter.convert(
                from_curr=req.from_currency.upper(),
                to_curr=req.to_currency.upper(),
                amount=req.amount,
                date=req.rate_date,
            )
            results.append({"index": i, "success": True, "result": result})
        except ValueError as e:
            errors.append({"index": i, "success": False, "error": str(e)})

    return {
        "total": len(requests),
        "successful": len(results),
        "failed": len(errors),
        "results": results,
        "errors": errors,
    }


# --- Triangulation ---


@app.post("/triangulate")
async def triangulate(
    from_currency: str,
    to_currency: str,
    amount: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Compare direct vs cross-rate conversion"""
    from_currency = from_currency.upper()
    to_currency = to_currency.upper()
    converter = await _build_converter(db_session, user_id)

    try:
        direct = converter.convert(from_currency, to_currency, amount)
    except ValueError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))

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
            "recommendation": "Use cross-rate" if savings > 0.1 else "Use direct rate",
        }
    return {
        "direct_conversion": direct,
        "cross_conversion": None,
        "recommendation": "No triangulation needed (same base currency)",
    }


# --- Historical Rates ---


@app.get("/rates/history/{from_currency}/{to_currency}")
async def get_historical_rates(
    from_currency: str,
    to_currency: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's historical exchange rates for a pair"""
    from_currency = from_currency.upper()
    to_currency = to_currency.upper()

    rates = [
        r
        for r in await crud.list_rates(db_session, user_id)
        if r.from_currency == from_currency and r.to_currency == to_currency
    ]
    if start_date:
        rates = [r for r in rates if r.effective_date >= start_date]
    if end_date:
        rates = [r for r in rates if r.effective_date <= end_date]

    rates.sort(key=lambda x: x.effective_date, reverse=True)

    return {
        "from_currency": from_currency,
        "to_currency": to_currency,
        "count": len(rates[:limit]),
        "rates": rates[:limit],
    }


# --- Currency Formatting ---


@app.get("/format/{currency_code}/{amount}")
async def format_amount(
    currency_code: str,
    amount: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Format amount with currency symbol"""
    currency = await crud.get_currency(db_session, user_id, currency_code.upper())

    decimal_places = currency.decimal_places
    formatted = f"{currency.symbol}{amount:,.{decimal_places}f}"

    return {
        "currency": currency.code,
        "symbol": currency.symbol,
        "amount": amount,
        "formatted": formatted,
        "decimal_places": decimal_places,
    }


# --- Multi-Currency Transaction Support ---


@app.post("/validate-transaction")
async def validate_multicurrency_transaction(
    transaction: MultiCurrencyTransaction,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Validate a multi-currency transaction against the caller's rates"""
    base_currency = transaction.base_currency.upper()

    try:
        await crud.get_currency(db_session, user_id, base_currency)
    except Exception:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail=f"Base currency {base_currency} not supported"
        )

    converter = await _build_converter(db_session, user_id)
    converted_lines = []
    total_base = Decimal("0")

    for line in transaction.lines:
        currency = line.get("currency", base_currency).upper()
        amount = Decimal(str(line.get("amount", 0)))

        # NOTE: the original called convert(base, currency) here, which summed
        # line-currency amounts into "total_in_base" nonsensically and raised
        # an unhandled ValueError (500) for unknown pairs. Direction fixed to
        # line-currency -> base, matching the response's "converted_to" intent.
        if currency != base_currency:
            try:
                result = converter.convert(from_curr=currency, to_curr=base_currency, amount=float(amount))
                converted_amount = result.converted_amount
            except ValueError:
                raise HTTPException(
                    status_code=status.HTTP_400_BAD_REQUEST,
                    detail=f"No exchange rate found for {currency} to {base_currency}",
                )
        else:
            converted_amount = float(amount)

        converted_lines.append(
            {
                "original_currency": currency,
                "original_amount": float(amount),
                "converted_amount": converted_amount,
                "converted_to": base_currency,
                "account": line.get("account"),
                "description": line.get("description"),
            }
        )
        total_base += Decimal(str(converted_amount))

    return {
        "valid": True,
        "base_currency": base_currency,
        "total_in_base": float(total_base),
        "line_count": len(transaction.lines),
        "lines": converted_lines,
    }


# --- Rate Alerts ---


@app.post("/alerts/rate")
async def create_rate_alert(
    from_currency: str,
    to_currency: str,
    target_rate: float,
    direction: Literal["above", "below", "any"],
    notification_url: Optional[str] = None,
):
    """Create an alert when exchange rate reaches target (stateless stub, unchanged)"""
    return {
        "alert_id": f"rate_alert_{from_currency}_{to_currency}",
        "from_currency": from_currency.upper(),
        "to_currency": to_currency.upper(),
        "target_rate": target_rate,
        "direction": direction,
        "notification_url": notification_url,
        "created_at": datetime.utcnow().isoformat(),
        "status": "active",
    }


# --- Statistics ---


@app.get("/stats")
async def get_currency_stats(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's currency service statistics"""
    return await crud.stats(db_session, user_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8092)
