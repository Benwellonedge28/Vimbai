"""Treasury operations: exact aggregation of any amount.

Amounts arrive as strings or numbers and leave as strings, so a book of
28-digit balances totals exactly instead of drifting through a float.
"""

from decimal import Decimal

from fastapi.testclient import TestClient
from main import app

client = TestClient(app)

HUGE = "123456789012345678901234567890.55"


def build_payload(**overrides):
    payload = {
        "company_id": "c1",
        "transactions": [
            {
                "transaction_id": "T1",
                "transaction_type": "inflow",
                "amount": "500000.55",
                "currency": "USD",
                "counterparty": "X",
                "status": "completed",
            },
            {
                "transaction_id": "T2",
                "transaction_type": "outflow",
                "amount": "250000.25",
                "currency": "USD",
                "counterparty": "Y",
                "status": "pending",
            },
            {
                "transaction_id": "T3",
                "transaction_type": "outflow",
                "amount": "12000.10",
                "currency": "USD",
                "counterparty": "Z",
                "status": "failed",
            },
            {
                "transaction_id": "T4",
                "transaction_type": "inflow",
                "amount": "3000.00",
                "currency": "USD",
                "counterparty": "W",
                "status": "failed",
            },
            {
                "transaction_id": "T5",
                "transaction_type": "outflow",
                "amount": "1000.00",
                "currency": "USD",
                "counterparty": "V",
                "status": "completed",
            },
        ],
        "bank_balances": {"ACC-A": "750000.75", "ACC-B": "400000.50"},
        "treasury_policy_limits": {"max_single": "500000", "min_cash": "1000000"},
    }
    payload.update(overrides)
    return payload


def test_health():
    assert client.get("/").json()["service"] == "treasury-operations"


def test_analyze_matches_the_documented_figures():
    body = client.post("/analyze", json=build_payload()).json()
    assert Decimal(body["total_cash"]) == Decimal("1150001.25")
    summary = body["transactions_summary"]
    assert Decimal(summary["total_inflows"]) == Decimal("503000.55")
    assert Decimal(summary["total_outflows"]) == Decimal("263000.35")
    assert Decimal(summary["net_flow"]) == Decimal("240000.20")
    assert summary["pending"] == 1
    assert summary["failed"] == 2
    # the compliance rate is a constant, never a measurement
    assert Decimal(body["policy_compliance"]["compliance_rate"]) == Decimal("0.95")
    assert Decimal(body["bank_balance_summary"]["largest_balance"]) == Decimal("750000.75")
    assert body["alerts"] == ["High failure rate: 2 failed transactions"]


def test_any_amount_totals_exactly():
    body = client.post(
        "/analyze",
        json=build_payload(
            bank_balances={"ZWG": HUGE, "USD": "0.45"},
            transactions=[
                {
                    "transaction_id": "H1",
                    "transaction_type": "inflow",
                    "amount": "1e30",
                    "currency": "ZWG",
                    "counterparty": "X",
                    "status": "completed",
                },
                {
                    "transaction_id": "H2",
                    "transaction_type": "outflow",
                    "amount": "1",
                    "currency": "ZWG",
                    "counterparty": "Y",
                    "status": "completed",
                },
            ],
        ),
    ).json()
    # .55 plus .45 lands exactly on a whole unit, cents intact
    assert Decimal(body["total_cash"]) == Decimal("123456789012345678901234567891.00")
    assert Decimal(body["bank_balance_summary"]["largest_balance"]) == Decimal(HUGE)
    assert Decimal(body["transactions_summary"]["net_flow"]) == Decimal("999999999999999999999999999999.00")


def test_failure_alert_needs_more_than_five_percent():
    twenty = [
        {
            "transaction_id": f"T{i}",
            "transaction_type": "inflow",
            "amount": "100",
            "currency": "USD",
            "counterparty": "X",
            "status": "failed" if i == 0 else "completed",
        }
        for i in range(20)
    ]
    body = client.post("/analyze", json=build_payload(transactions=twenty)).json()
    # 1 in 20 is not above 5%
    assert body["alerts"] == []

    body = client.post("/analyze", json=build_payload(transactions=twenty[:19])).json()
    assert body["alerts"] == ["High failure rate: 1 failed transactions"]


def test_unknown_types_and_failed_status_are_handled_as_before():
    body = client.post(
        "/analyze",
        json=build_payload(
            transactions=[
                {
                    "transaction_id": "U1",
                    "transaction_type": "transfer",
                    "amount": "500",
                    "currency": "USD",
                    "counterparty": "X",
                    "status": "failed",
                },
                {
                    "transaction_id": "U2",
                    "transaction_type": "Inflow",
                    "amount": "700",
                    "currency": "USD",
                    "counterparty": "X",
                    "status": "completed",
                },
            ],
        ),
    ).json()
    # neither "transfer" nor a capitalised "Inflow" counts towards a total,
    # but the failed status still raises its recommendation
    assert Decimal(body["transactions_summary"]["total_inflows"]) == Decimal("0.00")
    assert Decimal(body["transactions_summary"]["total_outflows"]) == Decimal("0.00")
    assert "Review failed transactions and resolve issues" in body["recommendations"]


def test_low_cash_recommendation_is_strict():
    exact = client.post("/analyze", json=build_payload(bank_balances={"A": "1000000"})).json()
    assert not any("low" in r for r in exact["recommendations"])

    below = client.post("/analyze", json=build_payload(bank_balances={"A": "999999.99"})).json()
    assert any("Cash position is low" in r for r in below["recommendations"])


def test_empty_book_is_zero_not_an_error():
    body = client.post(
        "/analyze",
        json={"company_id": "c1", "transactions": [], "bank_balances": {}, "treasury_policy_limits": {}},
    ).json()
    assert Decimal(body["total_cash"]) == Decimal("0")
    assert Decimal(body["bank_balance_summary"]["largest_balance"]) == Decimal("0")
    assert body["bank_balance_summary"]["bank_count"] == 0
