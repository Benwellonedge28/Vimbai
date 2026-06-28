"""
Treasury & Cash Management Service
Port: 8345
Cash position, liquidity management, and treasury operations
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime, date, timedelta
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Treasury & Cash Management Service", version="1.0.0")

class CashPosition(BaseModel):
    account_id: str
    account_name: str
    currency: str
    balance: float
    available_balance: float
    pending_transactions: float
    as_of_date: datetime

class CashForecastItem(BaseModel):
    date: date
    opening_balance: float
    expected_inflows: float
    expected_outflows: float
    closing_balance: float
    net_change: float

class CashPositionRequest(BaseModel):
    company_id: str
    account_ids: List[str]
    as_of_date: Optional[date] = None
    include_pending: bool = True

class CashPositionResponse(BaseModel):
    company_id: str
    as_of_date: datetime
    total_balance: float
    total_available: float
    total_pending: float
    currency_balances: Dict[str, float]
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
    minimum_cash_required: float
    recommended_buffer: float
    scenarios: Dict[str, List[CashForecastItem]]

class CashFlowOptimizationRequest(BaseModel):
    company_id: str
    target_cash_balance: float
    investment_options: List[Dict[str, Any]]
    risk_tolerance: str

class CashFlowOptimizationResponse(BaseModel):
    company_id: str
    current_cash: float
    target_cash: float
    excess_cash: float
    recommended_investments: List[Dict[str, Any]]
    expected_yield: float
    maturity_schedule: List[Dict[str, Any]]

class BankReconciliationRequest(BaseModel):
    company_id: str
    bank_account_id: str
    statement_date: date
    book_balance: float
    bank_statement_transactions: List[Dict[str, Any]]

class BankReconciliationResponse(BaseModel):
    company_id: str
    bank_account_id: str
    book_balance: float
    bank_balance: float
    adjusted_book_balance: float
    adjusted_bank_balance: float
    differences: List[Dict[str, Any]]
    outstanding_checks: List[Dict[str, Any]]
    deposits_in_transit: List[Dict[str, Any]]
    reconciliation_status: str

class LiquidityRatioRequest(BaseModel):
    company_id: str
    current_assets: float
    current_liabilities: float
    cash_and_equivalents: float
    marketable_securities: float
    inventory: float
    accounts_receivable: float

class LiquidityRatioResponse(BaseModel):
    company_id: str
    current_ratio: float
    quick_ratio: float
    cash_ratio: float
    defensive_interval: float
    liquidity_index: float
    assessment: str

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-cash", "version": "1.0.0"}

@app.post("/cash-position", response_model=CashPositionResponse)
async def get_cash_position(request: CashPositionRequest):
    logger.info("Fetching cash position", company=request.company_id, accounts=len(request.account_ids))

    accounts = []
    total_balance = 0.0
    total_available = 0.0
    total_pending = 0.0
    currency_balances: Dict[str, float] = {}

    for acc_id in request.account_ids:
        balance = 100000.0 * (hash(acc_id) % 100 + 1) / 100
        pending = balance * 0.05
        currency = "USD" if "USD" in acc_id else "EUR"

        accounts.append(CashPosition(
            account_id=acc_id,
            account_name=f"Account {acc_id[-4:]}",
            currency=currency,
            balance=round(balance, 2),
            available_balance=round(balance - pending, 2),
            pending_transactions=round(pending, 2),
            as_of_date=datetime.now()
        ))

        total_balance += balance
        total_available += balance - pending
        total_pending += pending
        currency_balances[currency] = currency_balances.get(currency, 0) + balance

    return CashPositionResponse(
        company_id=request.company_id,
        as_of_date=datetime.now(),
        total_balance=round(total_balance, 2),
        total_available=round(total_available, 2),
        total_pending=round(total_pending, 2),
        currency_balances={k: round(v, 2) for k, v in currency_balances.items()},
        accounts=accounts
    )

@app.post("/cash-forecast", response_model=CashForecastResponse)
async def forecast_cash_flow(request: CashForecastRequest):
    logger.info("Forecasting cash flow", company=request.company_id, start=request.start_date, end=request.end_date)

    projections = []
    current_balance = 500000.0
    days = (request.end_date - request.start_date).days + 1

    for i in range(days):
        forecast_date = request.start_date + timedelta(days=i)
        inflows = 50000 + (hash(str(forecast_date)) % 30000)
        outflows = 40000 + (hash(str(forecast_date) + "out") % 20000)
        closing = current_balance + inflows - outflows

        projections.append(CashForecastItem(
            date=forecast_date,
            opening_balance=round(current_balance, 2),
            expected_inflows=round(inflows, 2),
            expected_outflows=round(outflows, 2),
            closing_balance=round(closing, 2),
            net_change=round(inflows - outflows, 2)
        ))
        current_balance = closing

    scenarios = {
        "base": projections,
        "optimistic": [{"date": p.date, "opening_balance": p.opening_balance * 1.1, "expected_inflows": p.expected_inflows * 1.2, "expected_outflows": p.expected_outflows * 0.9, "closing_balance": p.closing_balance * 1.15, "net_change": p.net_change * 1.3} for p in projections],
        "pessimistic": [{"date": p.date, "opening_balance": p.opening_balance * 0.9, "expected_inflows": p.expected_inflows * 0.8, "expected_outflows": p.expected_outflows * 1.1, "closing_balance": p.closing_balance * 0.85, "net_change": p.net_change * 0.7} for p in projections]
    }

    return CashForecastResponse(
        company_id=request.company_id,
        forecast_period={"start": request.start_date, "end": request.end_date},
        projections=projections,
        minimum_cash_required=100000.0,
        recommended_buffer=50000.0,
        scenarios=scenarios
    )

@app.post("/cash-optimization", response_model=CashFlowOptimizationResponse)
async def optimize_cash_flow(request: CashFlowOptimizationRequest):
    logger.info("Optimizing cash flow", company=request.company_id, target=request.target_cash_balance)

    current_cash = 750000.0
    excess_cash = max(0, current_cash - request.target_cash_balance)

    investments = []
    for opt in request.investment_options:
        allocation = min(excess_cash * 0.3, opt.get("min_investment", 10000))
        if allocation > 0:
            investments.append({
                "instrument": opt.get("name", "T-Bill"),
                "amount": round(allocation, 2),
                "yield": opt.get("yield", 0.05),
                "maturity": opt.get("maturity_days", 90)
            })

    return CashFlowOptimizationResponse(
        company_id=request.company_id,
        current_cash=round(current_cash, 2),
        target_cash=round(request.target_cash_balance, 2),
        excess_cash=round(excess_cash, 2),
        recommended_investments=investments,
        expected_yield=0.045,
        maturity_schedule=[{"date": (date.today() + timedelta(days=m["maturity"])).isoformat(), "amount": m["amount"]} for m in investments]
    )

@app.post("/bank-reconciliation", response_model=BankReconciliationResponse)
async def reconcile_bank(request: BankReconciliationRequest):
    logger.info("Reconciling bank account", company=request.company_id, account=request.bank_account_id)

    outstanding_checks = [
        {"check_number": "1001", "amount": 5000.0, "date": "2024-01-15"},
        {"check_number": "1002", "amount": 3500.0, "date": "2024-01-18"}
    ]
    deposits_in_transit = [
        {"deposit_id": "D001", "amount": 12000.0, "date": "2024-01-28"}
    ]

    total_outstanding = sum(c["amount"] for c in outstanding_checks)
    total_deposits = sum(d["amount"] for d in deposits_in_transit)

    adjusted_book = request.book_balance - total_outstanding
    adjusted_bank = request.book_balance + total_deposits

    return BankReconciliationResponse(
        company_id=request.company_id,
        bank_account_id=request.bank_account_id,
        book_balance=round(request.book_balance, 2),
        bank_balance=round(request.book_balance + 500.0, 2),
        adjusted_book_balance=round(adjusted_book, 2),
        adjusted_bank_balance=round(adjusted_bank, 2),
        differences=[],
        outstanding_checks=outstanding_checks,
        deposits_in_transit=deposits_in_transit,
        reconciliation_status="balanced" if abs(adjusted_book - adjusted_bank) < 1 else "review_needed"
    )

@app.post("/liquidity-ratios", response_model=LiquidityRatioResponse)
async def calculate_liquidity_ratios(request: LiquidityRatioRequest):
    logger.info("Calculating liquidity ratios", company=request.company_id)

    current_ratio = request.current_assets / request.current_liabilities if request.current_liabilities else 0
    quick_ratio = (request.current_assets - request.inventory) / request.current_liabilities if request.current_liabilities else 0
    cash_ratio = (request.cash_and_equivalents + request.marketable_securities) / request.current_liabilities if request.current_liabilities else 0

    daily_cash_burn = 50000.0
    defensive_interval = (request.cash_and_equivalents + request.marketable_securities) / daily_cash_burn if daily_cash_burn else 0

    liquidity_index = (current_ratio * 0.3 + quick_ratio * 0.4 + cash_ratio * 0.3)

    assessment = "strong" if current_ratio > 2.0 else "adequate" if current_ratio > 1.5 else "weak"

    return LiquidityRatioResponse(
        company_id=request.company_id,
        current_ratio=round(current_ratio, 4),
        quick_ratio=round(quick_ratio, 4),
        cash_ratio=round(cash_ratio, 4),
        defensive_interval=round(defensive_interval, 2),
        liquidity_index=round(liquidity_index, 4),
        assessment=assessment
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8345)
