"""Cash Flow Statement Service - Book isolation tests."""

from fastapi.testclient import TestClient
from main import _statements, app

client = TestClient(app)

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}

STATEMENT = {
    "company_id": "co-1",
    "period_start": "2026-01-01T00:00:00Z",
    "period_end": "2026-01-31T00:00:00Z",
    "beginning_cash": 100.0,
    "operating_activities": [{"description": "sales", "amount": 500.0, "is_inflow": True}],
    "investing_activities": [],
    "financing_activities": [{"description": "loan repayment", "amount": 150.0, "is_inflow": False}],
}


def _cleanup():
    _statements.clear()


def test_generate_and_latest_roundtrip():
    _cleanup()
    r = client.post("/generate", json=STATEMENT)
    assert r.status_code == 200
    assert r.json()["net_change"] == 350.0
    assert r.json()["ending_cash"] == 450.0

    r = client.get("/latest/co-1")
    assert r.status_code == 200
    assert r.json()["company_id"] == "co-1"


def test_statements_isolated_between_books():
    _cleanup()
    a = TestClient(app)
    b = TestClient(app)
    assert a.post("/generate", json=STATEMENT, headers=BOOK_A).status_code == 200
    assert a.post("/generate", json=STATEMENT, headers=BOOK_A).status_code == 200
    assert b.post("/generate", json=STATEMENT, headers=BOOK_B).status_code == 200

    hist_a = a.get("/history/co-1", headers=BOOK_A).json()
    hist_b = b.get("/history/co-1", headers=BOOK_B).json()
    assert hist_a["total"] == 2
    assert hist_b["total"] == 1


def test_book_cannot_see_unscoped_or_other_books():
    _cleanup()
    a = TestClient(app)
    b = TestClient(app)
    assert a.post("/generate", json=STATEMENT).status_code == 200
    assert a.post("/generate", json=STATEMENT, headers=BOOK_A).status_code == 200

    assert b.get("/latest/co-1", headers=BOOK_B).status_code == 404
    assert client.get("/latest/co-1").status_code == 200
    hist_a = a.get("/history/co-1", headers=BOOK_A).json()
    assert hist_a["total"] == 1
