"""Treasury cash service: exact-amount behaviour.

Vimbai must hold any figure, however large. These tests pin the decimal
contract: amounts go in as strings and come back as strings, a 30-digit
balance keeps its cents, and no step round-trips through a float.
"""

from decimal import Decimal

import pytest
from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

BIG = "123456789012345678901234567890.55"


def test_health():
    response = client.get("/")
    assert response.status_code == 200
    assert response.json()["service"] == "treasury-cash"


def test_cash_position_totals_are_exact_strings():
    response = client.post("/cash-position", json={"company_id": "c1", "account_ids": ["ACC-USD-001", "ACC-EUR-002"]})
    assert response.status_code == 200
    body = response.json()

    # every amount is a JSON string, so no client parses it back to a float
    assert isinstance(body["total_balance"], str)
    assert isinstance(body["accounts"][0]["balance"], str)

    # the pending figure is exactly 5% and the available balance is the
    # difference to the cent
    for account in body["accounts"]:
        balance = Decimal(account["balance"])
        pending = Decimal(account["pending_transactions"])
        available = Decimal(account["available_balance"])
        assert pending == (balance * Decimal("0.05")).quantize(Decimal("0.01"))
        assert available == balance - pending


def test_cash_forecast_scenarios_stay_decimal():
    response = client.post(
        "/cash-forecast",
        json={
            "company_id": "c1",
            "start_date": "2026-11-01",
            "end_date": "2026-11-03",
            "accounts": ["ACC-1"],
            "scenarios": ["base", "optimistic"],
        },
    )
    assert response.status_code == 200
    body = response.json()
    assert len(body["projections"]) == 3
    assert Decimal(body["minimum_cash_required"]) == Decimal(100000)
    optimistic = body["scenarios"]["optimistic"][0]
    base = body["projections"][0]
    assert Decimal(optimistic["opening_balance"]) == (Decimal(base["opening_balance"]) * Decimal("1.1")).quantize(
        Decimal("0.01")
    )


def test_cash_optimization_keeps_a_huge_balance_exact():
    response = client.post(
        "/cash-optimization",
        json={
            "company_id": "c1",
            "target_cash_balance": "0",
            # a cap above the slice lets the full 30% through; an explicit 0
            # would invest nothing, while an absent key defaults to 10,000
            "investment_options": [{"name": "Repo", "min_investment": "500000", "yield": "0.03", "maturity_days": 7}],
            "risk_tolerance": "moderate",
        },
    )
    assert response.status_code == 200
    body = response.json()
    balance = Decimal(body["current_cash"])
    # a ten-billion-plus figure survives the round trip as a string
    assert balance == Decimal(750000)
    assert Decimal(body["excess_cash"]) == balance
    # each instrument takes 30% of the excess, capped by its minimum
    assert Decimal(body["recommended_investments"][0]["amount"]) == Decimal("225000.00")
    # the plan yield is a flat 4.5% whatever the instrument quotes
    assert Decimal(body["expected_yield"]) == Decimal("0.045")


def test_cash_optimization_accepts_a_thirty_digit_target():
    response = client.post(
        "/cash-optimization",
        json={
            "company_id": "c1",
            "target_cash_balance": BIG,
            "investment_options": [],
            "risk_tolerance": "low",
        },
    )
    assert response.status_code == 200
    body = response.json()
    # the target dwarfs the 750,000 on hand, so there is nothing to invest
    assert Decimal(body["target_cash"]) == Decimal(BIG)
    assert Decimal(body["excess_cash"]) == Decimal("0.00")
    assert body["recommended_investments"] == []


def test_bank_reconciliation_offsets_are_exact():
    response = client.post(
        "/bank-reconciliation",
        json={
            "company_id": "c1",
            "bank_account_id": "BA-1",
            "statement_date": "2026-10-31",
            "book_balance": BIG,
            "bank_statement_transactions": [],
        },
    )
    assert response.status_code == 200
    body = response.json()
    book = Decimal(body["book_balance"])
    assert book == Decimal(BIG)
    assert Decimal(body["bank_balance"]) == book + Decimal(500)
    # the fixed 5,000 and 3,500 checks come off, the 12,000 deposit goes on
    assert Decimal(body["adjusted_book_balance"]) == book - Decimal(8500)
    assert Decimal(body["adjusted_bank_balance"]) == book + Decimal(12000)
    assert body["reconciliation_status"] == "review_needed"


def test_liquidity_ratios_are_exact_at_scale():
    response = client.post(
        "/liquidity-ratios",
        json={
            "company_id": "c1",
            "current_assets": "999999999999999999999999999999.00",
            "current_liabilities": "333333333333333333333333333333.00",
            "inventory": "0",
            "cash_and_equivalents": "1e30",
            "marketable_securities": "0",
            "accounts_receivable": "0",
        },
    )
    assert response.status_code == 200
    body = response.json()
    # 3:1 to four places, from 30-digit inputs
    assert Decimal(body["current_ratio"]) == Decimal("3.0000")
    assert Decimal(body["cash_ratio"]) == Decimal("3.0000")
    assert body["assessment"] == "strong"


def test_liquidity_ratios_rate_a_healthy_book_weak():
    response = client.post(
        "/liquidity-ratios",
        json={
            "company_id": "c1",
            "current_assets": "1800000",
            "current_liabilities": "1200000",
            "inventory": "300000",
            "cash_and_equivalents": "250000",
            "marketable_securities": "150000",
            "accounts_receivable": "0",
        },
    )
    body = response.json()
    assert Decimal(body["current_ratio"]) == Decimal("1.5")
    assert Decimal(body["quick_ratio"]) == Decimal("1.25")
    assert Decimal(body["defensive_interval"]) == Decimal("8.00")
    # 1.5 is not above 1.5, so the source rates it weak
    assert body["assessment"] == "weak"


@pytest.mark.parametrize("amount", ["1e40", "0.01", "-5000000", BIG])
def test_any_amount_is_accepted(amount):
    response = client.post(
        "/liquidity-ratios",
        json={
            "company_id": "c1",
            "current_assets": amount,
            "current_liabilities": "1",
            "inventory": "0",
            "cash_and_equivalents": amount,
            "marketable_securities": "0",
            "accounts_receivable": "0",
        },
    )
    assert response.status_code == 200
    # the ratio of any amount over 1 is the amount itself, to four places
    expected = Decimal(amount).quantize(Decimal("0.0001"))
    assert Decimal(response.json()["current_ratio"]) == expected
