"""Rolling forecast tests: persistence, Book scoping and the original quirks."""

import sys
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fake_neo4j
import main  # bootstraps the package alias
from rolling_forecast_service.database import Neo4jConnector

app = main.app

client = TestClient(app)
H = {"X-User-Id": "user-1", "X-Book-ID": "book-1"}


@pytest.fixture(autouse=True)
def _fake_db(monkeypatch):
    fake_neo4j.reset()
    monkeypatch.setattr(
        Neo4jConnector,
        "get_driver",
        classmethod(lambda cls: fake_neo4j.FakeDriver(fake_neo4j.session())),
    )
    yield


def _create(name="FY26 revenue", metric="revenue", periods=None, headers=H):
    payload = (
        periods
        if periods is not None
        else [
            {"period": "2026-01", "forecast_value": 10_000.0},
            {"period": "2026-02", "forecast_value": 12_000.0},
        ]
    )
    resp = client.post(
        "/forecasts",
        params={"name": name, "metric": metric},
        json=payload,
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_create_stamps_book_and_first_period():
    body = _create()
    assert body["name"] == "FY26 revenue"
    assert body["frequency"] == "monthly"
    assert body["horizon_periods"] == 12
    assert body["current_period"] == "2026-01"
    assert [p["period"] for p in body["periods"]] == ["2026-01", "2026-02"]


def test_invalid_frequency_is_rejected():
    resp = client.post(
        "/forecasts",
        params={"name": "x", "metric": "revenue", "frequency": "daily"},
        json=[],
        headers=H,
    )
    assert resp.status_code == 400
    assert "Invalid frequency" in resp.json()["detail"]


def test_forecast_survives_a_new_request_and_lists():
    created = _create()
    listed = client.get("/forecasts", headers=H).json()
    assert [f["id"] for f in listed] == [created["id"]]
    fetched = client.get(f"/forecasts/{created['id']}", headers=H)
    assert fetched.status_code == 200
    assert fetched.json()["metric"] == "revenue"


def test_metric_filter_narrows_the_list():
    _create(name="rev", metric="revenue")
    _create(name="cost", metric="expenses")
    only_cost = client.get("/forecasts", params={"metric": "expenses"}, headers=H).json()
    assert len(only_cost) == 1
    assert only_cost[0]["metric"] == "expenses"


def test_update_actual_computes_variance():
    created = _create()
    resp = client.post(
        f"/forecasts/{created['id']}/update-actual",
        params={"period": "2026-01", "actual_value": 11_000.0},
        headers=H,
    )
    assert resp.status_code == 200
    assert resp.json()["variance"] == pytest.approx(1_000.0)

    stored = client.get(f"/forecasts/{created['id']}", headers=H).json()
    period = next(p for p in stored["periods"] if p["period"] == "2026-01")
    assert period["is_actual"] is True
    assert period["variance_pct"] == pytest.approx(10.0)


def test_unknown_period_reports_null_variance():
    """Original behaviour: a period that is not in the forecast is a no-op."""
    created = _create()
    resp = client.post(
        f"/forecasts/{created['id']}/update-actual",
        params={"period": "2026-09", "actual_value": 5.0},
        headers=H,
    )
    assert resp.status_code == 200
    assert resp.json()["variance"] is None


def test_zero_forecast_value_keeps_variance_pct_at_zero():
    created = _create(periods=[{"period": "2026-01", "forecast_value": 0}])
    resp = client.post(
        f"/forecasts/{created['id']}/update-actual",
        params={"period": "2026-01", "actual_value": 500.0},
        headers=H,
    )
    assert resp.status_code == 200
    stored = client.get(f"/forecasts/{created['id']}", headers=H).json()
    period = stored["periods"][0]
    assert period["variance"] == pytest.approx(500.0)
    assert period["variance_pct"] == 0


def test_roll_appends_without_trimming_the_horizon():
    created = _create()
    resp = client.post(
        f"/forecasts/{created['id']}/roll",
        json=[{"period": "2026-03", "forecast_value": 13_000.0}],
        headers=H,
    )
    assert resp.status_code == 200
    assert resp.json()["total_periods"] == 3


def test_cross_book_access_is_a_404():
    created = _create()
    other = {"X-User-Id": "user-2", "X-Book-ID": "book-2"}
    assert client.get("/forecasts", headers=other).json() == []
    assert client.get(f"/forecasts/{created['id']}", headers=other).status_code == 404
    assert (
        client.post(
            f"/forecasts/{created['id']}/roll",
            json=[{"period": "2026-03", "forecast_value": 1.0}],
            headers=other,
        ).status_code
        == 404
    )


def test_missing_user_header_is_rejected():
    assert client.get("/forecasts").status_code == 422
