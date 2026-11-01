"""
Treasury & Cash Management Service
Port: 8345
Cash position, liquidity management, and treasury operations
"""

from datetime import date, datetime, timedelta
from decimal import ROUND_HALF_EVEN, Decimal, getcontext, localcontext
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Treasury & Cash Management Service", version="1.0.0")

CENT = Decimal("0.01")

# 200 significant digits of headroom. The default 28-digit decimal context is
# itself too small for a group balance: a 30-digit figure raises
# InvalidOperation on quantize before any arithmetic happens.
getcontext().prec = 200
getcontext().rounding = ROUND_HALF_EVEN


def _working_precision(*values: Decimal) -> int:
    """Significant digits needed to work on these amounts without rounding.

    Widens automatically, so an amount far larger than the default context can
    hold (a hyperinflated currency, a 400-digit ledger) is still computed
    exactly rather than silently truncated.
    """
    needed = 60
    for value in values:
        if not isinstance(value, Decimal):
            continue
        if not value.is_finite():
            continue
        span = value.adjusted() + len(value.as_tuple().digits)
        needed = max(needed, span + 40)
    return needed


def round_money(value: Decimal, places: int = 2) -> Decimal:
    """Round an amount to a currency scale, half to even.

    Vimbai holds any figure at any magnitude: the models take Decimal (or a
    string), so a 30-digit balance never passes through a binary float, and
    this rounds only at the point of presentation.
    """
    with localcontext() as ctx:
        ctx.prec = _working_precision(value)
        ctx.rounding = ROUND_HALF_EVEN
        return value.quantize(Decimal(1).scaleb(-places), rounding=ROUND_HALF_EVEN)


def divide_money(numerator: Decimal, denominator: Decimal) -> Decimal:
    """Divide two amounts at a precision wide enough for both."""
    with localcontext() as ctx:
        ctx.prec = _working_precision(numerator, denominator)
        ctx.rounding = ROUND_HALF_EVEN
        return numerator / denominator


def to_money(value: Any, default: str = "0") -> Decimal:
    """Accept a Decimal, a string or a number from an untyped options list."""
    if isinstance(value, Decimal):
        return value
    if value is None:
        return Decimal(default)
    return Decimal(str(value))


class CashPosition(BaseModel):
    account_id: str
    account_name: str
    currency: str
    balance: Decimal
    available_balance: Decimal
    pending_transactions: Decimal
    as_of_date: datetime


class CashForecastItem(BaseModel):
    date: date
    opening_balance: Decimal
    expected_inflows: Decimal
    expected_outflows: Decimal
    closing_balance: Decimal
    net_change: Decimal


class CashPositionRequest(BaseModel):
    company_id: str
    account_ids: List[str]
    as_of_date: Optional[date] = None
    include_pending: bool = True


class CashPositionResponse(BaseModel):
    company_id: str
    as_of_date: datetime
    total_balance: Decimal
    total_available: Decimal
    total_pending: Decimal
    currency_balances: Dict[str, Decimal]
    accounts: List[CashPosition]


class CashForecastRequest(BaseModel):
    company_id: str
    start_date: date
    end_date: date
    accounts: List[str]
    scenarios: List[str]


class CashForecastResponse(BaseModel):
    company_id: str
    forecast_period: Dict[str, date]
    projections: List[CashForecastItem]
    minimum_cash_required: Decimal
    recommended_buffer: Decimal
    scenarios: Dict[str, List[CashForecastItem]]


class CashFlowOptimizationRequest(BaseModel):
    company_id: str
    target_cash_balance: Decimal
    investment_options: List[Dict[str, Any]]
    risk_tolerance: str


class CashFlowOptimizationResponse(BaseModel):
    company_id: str
    current_cash: Decimal
    target_cash: Decimal
    excess_cash: Decimal
    recommended_investments: List[Dict[str, Any]]
    expected_yield: Decimal
    maturity_schedule: List[Dict[str, Any]]


class BankReconciliationRequest(BaseModel):
    company_id: str
    bank_account_id: str
    statement_date: date
    book_balance: Decimal
    bank_statement_transactions: List[Dict[str, Any]]


class BankReconciliationResponse(BaseModel):
    company_id: str
    bank_account_id: str
    book_balance: Decimal
    bank_balance: Decimal
    adjusted_book_balance: Decimal
    adjusted_bank_balance: Decimal
    differences: List[Dict[str, Any]]
    outstanding_checks: List[Dict[str, Any]]
    deposits_in_transit: List[Dict[str, Any]]
    reconciliation_status: str


class LiquidityRatioRequest(BaseModel):
    company_id: str
    current_assets: Decimal
    current_liabilities: Decimal
    cash_and_equivalents: Decimal
    marketable_securities: Decimal
    inventory: Decimal
    accounts_receivable: Decimal


class LiquidityRatioResponse(BaseModel):
    company_id: str
    current_ratio: Decimal
    quick_ratio: Decimal
    cash_ratio: Decimal
    defensive_interval: Decimal
    liquidity_index: Decimal
    assessment: str


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-cash", "version": "1.0.0"}


@app.post("/cash-position", response_model=CashPositionResponse)
async def get_cash_position(request: CashPositionRequest):
    logger.info("Fetching cash position", company=request.company_id, accounts=len(request.account_ids))

    accounts = []
    total_balance = Decimal("0")
    total_available = Decimal("0")
    total_pending = Decimal("0")
    currency_balances: Dict[str, Decimal] = {}

    for acc_id in request.account_ids:
        balance = Decimal(100000) * (hash(acc_id) % 100 + 1) / 100
        pending = balance * Decimal("0.05")
        currency = "USD" if "USD" in acc_id else "EUR"

        accounts.append(
            CashPosition(
                account_id=acc_id,
                account_name=f"Account {acc_id[-4:]}",
                currency=currency,
                balance=round_money(balance),
                available_balance=round_money(balance - pending),
                pending_transactions=round_money(pending),
                as_of_date=datetime.now(),
            )
        )

        total_balance += balance
        total_available += balance - pending
        total_pending += pending
        currency_balances[currency] = currency_balances.get(currency, Decimal("0")) + balance

    return CashPositionResponse(
        company_id=request.company_id,
        as_of_date=datetime.now(),
        total_balance=round_money(total_balance),
        total_available=round_money(total_available),
        total_pending=round_money(total_pending),
        currency_balances={k: round_money(v) for k, v in currency_balances.items()},
        accounts=accounts,
    )


@app.post("/cash-forecast", response_model=CashForecastResponse)
async def forecast_cash_flow(request: CashForecastRequest):
    logger.info("Forecasting cash flow", company=request.company_id, start=request.start_date, end=request.end_date)

    projections = []
    current_balance = Decimal(500000)
    days = (request.end_date - request.start_date).days + 1

    for i in range(days):
        forecast_date = request.start_date + timedelta(days=i)
        inflows = Decimal(50000 + (hash(str(forecast_date)) % 30000))
        outflows = Decimal(40000 + (hash(str(forecast_date) + "out") % 20000))
        closing = current_balance + inflows - outflows

        projections.append(
            CashForecastItem(
                date=forecast_date,
                opening_balance=round_money(current_balance),
                expected_inflows=round_money(inflows),
                expected_outflows=round_money(outflows),
                closing_balance=round_money(closing),
                net_change=round_money(inflows - outflows),
            )
        )
        current_balance = closing

    scenarios = {
        "base": projections,
        "optimistic": [
            {
                "date": p.date,
                "opening_balance": round_money(p.opening_balance * Decimal("1.1")),
                "expected_inflows": round_money(p.expected_inflows * Decimal("1.2")),
                "expected_outflows": round_money(p.expected_outflows * Decimal("0.9")),
                "closing_balance": round_money(p.closing_balance * Decimal("1.15")),
                "net_change": round_money(p.net_change * Decimal("1.3")),
            }
            for p in projections
        ],
        "pessimistic": [
            {
                "date": p.date,
                "opening_balance": round_money(p.opening_balance * Decimal("0.9")),
                "expected_inflows": round_money(p.expected_inflows * Decimal("0.8")),
                "expected_outflows": round_money(p.expected_outflows * Decimal("1.1")),
                "closing_balance": round_money(p.closing_balance * Decimal("0.85")),
                "net_change": round_money(p.net_change * Decimal("0.7")),
            }
            for p in projections
        ],
    }

    return CashForecastResponse(
        company_id=request.company_id,
        forecast_period={"start": request.start_date, "end": request.end_date},
        projections=projections,
        minimum_cash_required=Decimal(100000),
        recommended_buffer=Decimal(50000),
        scenarios=scenarios,
    )


@app.post("/cash-optimization", response_model=CashFlowOptimizationResponse)
async def optimize_cash_flow(request: CashFlowOptimizationRequest):
    logger.info("Optimizing cash flow", company=request.company_id, target=request.target_cash_balance)

    current_cash = Decimal(750000)
    excess_cash = max(Decimal("0"), current_cash - request.target_cash_balance)

    investments = []
    for opt in request.investment_options:
        # the source treats min_investment as a CAP on each slice, and gives
        # every instrument its own 30% of the excess
        allocation = min(excess_cash * Decimal("0.3"), to_money(opt.get("min_investment"), "10000"))
        if allocation > 0:
            investments.append(
                {
                    "instrument": opt.get("name", "T-Bill"),
                    "amount": round_money(allocation),
                    "yield": to_money(opt.get("yield"), "0.05"),
                    "maturity": opt.get("maturity_days", 90),
                }
            )

    return CashFlowOptimizationResponse(
        company_id=request.company_id,
        current_cash=round_money(current_cash),
        target_cash=round_money(request.target_cash_balance),
        excess_cash=round_money(excess_cash),
        recommended_investments=investments,
        expected_yield=Decimal("0.045"),
        maturity_schedule=[
            {"date": (date.today() + timedelta(days=m["maturity"])).isoformat(), "amount": m["amount"]}
            for m in investments
        ],
    )


@app.post("/bank-reconciliation", response_model=BankReconciliationResponse)
async def reconcile_bank(request: BankReconciliationRequest):
    logger.info("Reconciling bank account", company=request.company_id, account=request.bank_account_id)

    outstanding_checks = [
        {"check_number": "1001", "amount": Decimal(5000), "date": "2024-01-15"},
        {"check_number": "1002", "amount": Decimal(3500), "date": "2024-01-18"},
    ]
    deposits_in_transit = [{"deposit_id": "D001", "amount": Decimal(12000), "date": "2024-01-28"}]

    total_outstanding = sum((c["amount"] for c in outstanding_checks), Decimal("0"))
    total_deposits = sum((d["amount"] for d in deposits_in_transit), Decimal("0"))

    adjusted_book = request.book_balance - total_outstanding
    adjusted_bank = request.book_balance + total_deposits

    return BankReconciliationResponse(
        company_id=request.company_id,
        bank_account_id=request.bank_account_id,
        book_balance=round_money(request.book_balance),
        bank_balance=round_money(request.book_balance + Decimal(500)),
        adjusted_book_balance=round_money(adjusted_book),
        adjusted_bank_balance=round_money(adjusted_bank),
        differences=[],
        outstanding_checks=outstanding_checks,
        deposits_in_transit=deposits_in_transit,
        reconciliation_status="balanced" if abs(adjusted_book - adjusted_bank) < 1 else "review_needed",
    )


@app.post("/liquidity-ratios", response_model=LiquidityRatioResponse)
async def calculate_liquidity_ratios(request: LiquidityRatioRequest):
    logger.info("Calculating liquidity ratios", company=request.company_id)

    zero = Decimal("0")
    has_liabilities = request.current_liabilities != 0
    liquid = request.cash_and_equivalents + request.marketable_securities

    current_ratio = request.current_assets / request.current_liabilities if has_liabilities else zero
    quick_ratio = (
        (request.current_assets - request.inventory) / request.current_liabilities if has_liabilities else zero
    )
    cash_ratio = liquid / request.current_liabilities if has_liabilities else zero

    daily_cash_burn = Decimal(50000)
    defensive_interval = liquid / daily_cash_burn if daily_cash_burn else zero

    liquidity_index = current_ratio * Decimal("0.3") + quick_ratio * Decimal("0.4") + cash_ratio * Decimal("0.3")

    assessment = "strong" if current_ratio > Decimal(2) else "adequate" if current_ratio > Decimal("1.5") else "weak"

    return LiquidityRatioResponse(
        company_id=request.company_id,
        current_ratio=round_money(current_ratio, 4),
        quick_ratio=round_money(quick_ratio, 4),
        cash_ratio=round_money(cash_ratio, 4),
        defensive_interval=round_money(defensive_interval),
        liquidity_index=round_money(liquidity_index, 4),
        assessment=assessment,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8345)
