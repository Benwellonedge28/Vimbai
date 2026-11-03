"""Performance benchmarking: Book-scoped Neo4j persistence.

Metrics, results and peer data used to be module-level lists, shared across
tenants and lost on restart. The percentile maths is unchanged; where the
records live is not.
"""

import benchmark_store as store
import pytest
from fake_neo4j import FakeDriver, FakeSession
from fastapi.testclient import TestClient
from main import app

BOOK_ONE = {"X-User-Id": "user-1", "X-Book-ID": "book-1"}
BOOK_TWO = {"X-User-Id": "user-2", "X-Book-ID": "book-2"}


@pytest.fixture
def client():
    store.set_driver(FakeDriver(FakeSession()))
    yield TestClient(app)
    store.set_driver(None)


def _metric(client, headers=BOOK_ONE, name="Gross margin"):
    resp = client.post(
        "/metrics",
        params={
            "name": name,
            "category": "financial",
            "unit": "percentage",
            "industry_median": 40,
            "industry_top_quartile": 60,
            "industry_bottom_quartile": 20,
        },
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    assert client.get("/health").json()["service"] == "performance-benchmarking-service"


def test_metric_round_trips(client):
    created = _metric(client)
    assert created["industry_median"] == 40
    listed = client.get("/metrics", headers=BOOK_ONE).json()
    assert [m["id"] for m in listed] == [created["id"]]
    assert client.get("/metrics", params={"category": "customer"}, headers=BOOK_ONE).json() == []


def test_benchmark_ratings_are_unchanged(client):
    metric = _metric(client)
    top = client.post(
        "/benchmark", params={"org_value": 65, "metric_id": metric["id"], "period": "2026-Q3"}, headers=BOOK_ONE
    ).json()
    assert top["rating"] == "top_quartile"
    assert top["percentile_rank"] >= 75

    low = client.post(
        "/benchmark", params={"org_value": 10, "metric_id": metric["id"], "period": "2026-Q3"}, headers=BOOK_ONE
    ).json()
    assert low["rating"] == "below_average"

    stored = client.get("/results", params={"metric_id": metric["id"]}, headers=BOOK_ONE).json()
    assert len(stored) == 2


def test_benchmark_needs_a_metric_in_the_same_book(client):
    metric = _metric(client, BOOK_ONE)
    missing = client.post(
        "/benchmark", params={"org_value": 10, "metric_id": metric["id"], "period": "2026-Q3"}, headers=BOOK_TWO
    )
    assert missing.status_code == 404


def test_peer_comparison_is_stored_per_book(client):
    metric = _metric(client)
    peer = client.post(
        "/peer-comparisons",
        params={"metric_id": metric["id"], "organization_name": "Peer Ltd", "value": 55, "period": "2026-Q3"},
        headers=BOOK_ONE,
    )
    assert peer.status_code == 200
    mine = client.get(f"/peer-comparisons/{metric['id']}", headers=BOOK_ONE).json()
    assert [p["organization_name"] for p in mine] == ["Peer Ltd"]
    assert client.get(f"/peer-comparisons/{metric['id']}", headers=BOOK_TWO).json() == []


def test_one_book_never_sees_another_books_records(client):
    _metric(client, BOOK_ONE)
    assert client.get("/metrics", headers=BOOK_TWO).json() == []
    assert client.get("/results", headers=BOOK_TWO).json() == []
