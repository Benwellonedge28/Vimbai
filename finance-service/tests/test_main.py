"""
Vimbai Finance Service - Test Suite
Tests: budget CRUD, forecast CRUD, scenario engine, Book isolation (X-Book-ID scoping).
"""

import os
from datetime import datetime, timedelta, timezone

import pytest
from fastapi.testclient import TestClient

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")
os.environ.setdefault("NEO4J_PASSWORD", "test-password")

import finance_service.dependencies as deps
from finance_service.dependencies import get_db_session
from main import app

client = TestClient(app)

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}


# ---------------------------------------------------------------------------
# Fake Neo4j async session: interprets the exact Cypher patterns crud uses.
# ---------------------------------------------------------------------------
class NeoStr(str):
    def iso_format(self):
        return str(self)


class Counters:
    def __init__(self, nodes_deleted=0):
        self.nodes_deleted = nodes_deleted


class Summary:
    def __init__(self, counters):
        self.counters = counters


class FakeResult:
    def __init__(self, records=None, nodes_deleted=0):
        self._records = records or []
        self._summary = Summary(Counters(nodes_deleted))

    async def single(self):
        return self._records[0] if self._records else None

    def __aiter__(self):
        self._iter = iter(self._records)
        return self

    async def __anext__(self):
        try:
            return next(self._iter)
        except StopIteration:
            raise StopAsyncIteration

    def consume(self):
        return self._summary


class FakeSession:
    def __init__(self):
        self.budgets = []
        self.forecasts = []

    @staticmethod
    def _visible(node, params):
        return params.get("book_id") is None or node.get("book_id") == params.get("book_id")

    async def run(self, query, params=None, **kw):
        merged = dict(params or {})
        merged.update(kw)

        # --- Budget family ---
        if "CREATE (b:Budget {" in query:
            budget = {
                "id": merged["id"],
                "book_id": merged.get("book_id"),
                "name": merged["name"],
                "start_date": merged["start_date"],
                "end_date": merged["end_date"],
                "currency": merged["currency"],
                "description": merged.get("description"),
                "created_at": NeoStr(merged["created_at"]),
                "updated_at": NeoStr(merged["updated_at"]),
            }
            self.budgets.append(budget)
            return FakeResult([{"b": budget}])

        if "MATCH (b:Budget {id: $budget_id})" in query and "DETACH DELETE b" in query:
            found = [b for b in self.budgets if b["id"] == merged["budget_id"] and self._visible(b, merged)]
            for b in found:
                self.budgets.remove(b)
            return FakeResult(nodes_deleted=len(found))

        if "MATCH (b:Budget {id: $budget_id})" in query and "SET" in query:
            found = [b for b in self.budgets if b["id"] == merged["budget_id"] and self._visible(b, merged)]
            if not found:
                return FakeResult()
            b = found[0]
            for key in ("name", "start_date", "end_date", "currency", "description", "updated_at"):
                if key in merged:
                    b[key] = NeoStr(merged[key]) if key == "updated_at" else merged[key]
            return FakeResult([{"b": b}])

        if "MATCH (u:User {id: $user_id})-[:OWNS_BUDGET]->(b:Budget)" in query:
            found = [b for b in self.budgets if self._visible(b, merged)]
            found.sort(key=lambda b: b.get("start_date", ""), reverse=True)
            return FakeResult([{"b": b, "budget_items": []} for b in found])

        if "MATCH (b:Budget {id: $budget_id})" in query and "OPTIONAL MATCH" in query:
            found = [b for b in self.budgets if b["id"] == merged["budget_id"] and self._visible(b, merged)]
            if not found:
                return FakeResult()
            return FakeResult([{"b": found[0], "budget_items": []}])

        # --- Forecast family ---
        if "CREATE (f:Forecast {" in query:
            forecast = {
                "id": merged["id"],
                "book_id": merged.get("book_id"),
                "user_id": merged["user_id"],
                "name": merged["name"],
                "description": merged.get("description"),
                "start_date": merged["start_date"],
                "end_date": merged["end_date"],
                "interval": merged["interval"],
                "values": merged["values"],
                "is_baseline": bool(merged.get("is_baseline", True)),
                "parent_forecast_id": merged.get("parent_forecast_id"),
                "created_at": NeoStr(merged["created_at"]),
                "updated_at": NeoStr(merged["updated_at"]),
            }
            self.forecasts.append(forecast)
            return FakeResult([{"f": forecast}])

        if "MATCH (f:Forecast {id: $forecast_id})" in query and "DETACH DELETE f" in query:
            found = [f for f in self.forecasts if f["id"] == merged["forecast_id"] and self._visible(f, merged)]
            for f in found:
                self.forecasts.remove(f)
            return FakeResult(nodes_deleted=len(found))

        if "MATCH (f:Forecast {id: $forecast_id})" in query and "SET" in query:
            found = [f for f in self.forecasts if f["id"] == merged["forecast_id"] and self._visible(f, merged)]
            if not found:
                return FakeResult()
            f = found[0]
            for key in ("name", "description", "start_date", "end_date", "interval", "values", "updated_at"):
                if key in merged:
                    f[key] = NeoStr(merged[key]) if key == "updated_at" else merged[key]
            return FakeResult([{"f": f}])

        if "MATCH (u:User {id: $user_id})-[:OWNS_FORECAST]->(f:Forecast)" in query:
            found = [f for f in self.forecasts if self._visible(f, merged)]
            found.sort(key=lambda f: f.get("name", ""))
            return FakeResult([{"f": f} for f in found])

        if "MATCH (f:Forecast {id: $forecast_id})" in query:
            found = [f for f in self.forecasts if f["id"] == merged["forecast_id"] and self._visible(f, merged)]
            if not found:
                return FakeResult()
            return FakeResult([{"f": found[0]}])

        raise AssertionError(f"FakeSession: unhandled query: {query[:80]!r}")


@pytest.fixture
def fake_session():
    return FakeSession()


@pytest.fixture(autouse=True)
def override_db(fake_session):
    async def _get():
        yield fake_session

    app.dependency_overrides[get_db_session] = _get
    yield
    app.dependency_overrides.pop(get_db_session, None)


@pytest.fixture
def auth_headers():
    import jwt as pyjwt

    token = pyjwt.encode(
        {
            "user_id": "test-user-id",
            "username": "testuser",
            "role": "admin",
            "permissions": ["*.*"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


def _budget_payload(name="Q1 2026 Operating Budget"):
    return {
        "name": name,
        "start_date": "2026-01-01T00:00:00",
        "end_date": "2026-03-31T00:00:00",
        "currency": "USD",
        "description": "Operating budget for Q1",
    }


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestAuth:
    def test_create_budget_no_auth(self):
        response = client.post("/budgets/", json=_budget_payload())
        assert response.status_code in [401, 403]

    def test_list_budgets_no_auth(self):
        response = client.get("/budgets/")
        assert response.status_code in [401, 403]


class TestBudgetCRUD:
    def test_create_and_get_budget(self, auth_headers):
        r = client.post("/budgets/", json=_budget_payload(), headers=auth_headers)
        assert r.status_code == 201, r.text
        budget_id = r.json()["id"]

        r = client.get(f"/budgets/{budget_id}", headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["name"] == "Q1 2026 Operating Budget"

    def test_list_budgets(self, auth_headers):
        client.post("/budgets/", json=_budget_payload("Budget One"), headers=auth_headers)
        client.post("/budgets/", json=_budget_payload("Budget Two"), headers=auth_headers)
        r = client.get("/budgets/", headers=auth_headers)
        assert r.status_code == 200
        assert len(r.json()) == 2

    def test_update_budget(self, auth_headers):
        r = client.post("/budgets/", json=_budget_payload(), headers=auth_headers)
        budget_id = r.json()["id"]
        r = client.put(f"/budgets/{budget_id}", json={"name": "Q1 2026 Revised"}, headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["name"] == "Q1 2026 Revised"

    def test_delete_budget(self, auth_headers):
        r = client.post("/budgets/", json=_budget_payload(), headers=auth_headers)
        budget_id = r.json()["id"]
        r = client.delete(f"/budgets/{budget_id}", headers=auth_headers)
        assert r.status_code == 204

    def test_unknown_budget_404(self, auth_headers):
        r = client.get("/budgets/nope-404", headers=auth_headers)
        assert r.status_code == 404

    def test_end_date_before_start_rejected(self, auth_headers):
        r = client.post(
            "/budgets/",
            json={**_budget_payload(), "start_date": "2026-03-31T00:00:00", "end_date": "2026-01-01T00:00:00"},
            headers=auth_headers,
        )
        assert r.status_code == 422


class TestForecastCRUD:
    def test_generate_baseline_forecast(self, auth_headers, fake_session):
        r = client.post("/forecasts/baseline", headers=auth_headers)
        assert r.status_code == 201, r.text
        body = r.json()
        assert body["is_baseline"] is True
        assert len(body["values"]) > 0

        # stored node carries the Book stamp of the request context
        stored = fake_session.forecasts[0]
        assert stored["book_id"] is None  # personal (unscoped) request

    def test_get_forecast(self, auth_headers, fake_session):
        r = client.post("/forecasts/baseline", headers=auth_headers)
        forecast_id = r.json()["id"]
        r = client.get(f"/forecasts/{forecast_id}", headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["id"] == forecast_id

    def test_list_forecasts(self, auth_headers):
        client.post("/forecasts/baseline", headers=auth_headers)
        r = client.get("/forecasts/", headers=auth_headers)
        assert r.status_code == 200
        assert len(r.json()) == 1

    def test_apply_scenario(self, auth_headers):
        r = client.post("/forecasts/baseline", headers=auth_headers)
        forecast_id = r.json()["id"]
        r = client.post(
            f"/forecasts/{forecast_id}/scenario",
            json={"name": "Bull case", "revenue_growth_rate": 0.10},
            headers=auth_headers,
        )
        assert r.status_code == 201, r.text
        assert r.json()["is_baseline"] is False
        assert r.json()["parent_forecast_id"] == forecast_id

    def test_apply_scenario_to_non_baseline_rejected(self, auth_headers):
        r = client.post("/forecasts/baseline", headers=auth_headers)
        forecast_id = r.json()["id"]
        r = client.post(
            f"/forecasts/{forecast_id}/scenario",
            json={"name": "Stacked", "revenue_growth_rate": 0.05},
            headers=auth_headers,
        )
        scenario_id = r.json()["id"]
        r = client.post(
            f"/forecasts/{scenario_id}/scenario",
            json={"name": "Stacked twice", "revenue_growth_rate": 0.05},
            headers=auth_headers,
        )
        assert r.status_code == 400  # can only apply scenarios to baselines

    def test_update_forecast(self, auth_headers):
        r = client.post("/forecasts/baseline", headers=auth_headers)
        forecast_id = r.json()["id"]
        r = client.put(f"/forecasts/{forecast_id}", json={"name": "Renamed Forecast"}, headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["name"] == "Renamed Forecast"

    def test_delete_forecast(self, auth_headers):
        r = client.post("/forecasts/baseline", headers=auth_headers)
        forecast_id = r.json()["id"]
        r = client.delete(f"/forecasts/{forecast_id}", headers=auth_headers)
        assert r.status_code == 204

    def test_unknown_forecast_404(self, auth_headers):
        r = client.get("/forecasts/nope-404", headers=auth_headers)
        assert r.status_code == 404


class TestBookIsolation:
    def test_budget_stamped_with_book_id(self, auth_headers, fake_session):
        client.post("/budgets/", json=_budget_payload(), headers={**auth_headers, **BOOK_A})
        assert fake_session.budgets[0]["book_id"] == "book-aaa-111"

        # unscoped (personal) budgets keep book_id = None
        client.post("/budgets/", json=_budget_payload("Personal Budget"), headers=auth_headers)
        assert fake_session.budgets[1]["book_id"] is None

    def test_forecast_stamped_with_book_id(self, auth_headers, fake_session):
        client.post("/forecasts/baseline", headers={**auth_headers, **BOOK_B})
        assert fake_session.forecasts[0]["book_id"] == "book-bbb-222"

    def test_budget_hidden_from_other_book(self, auth_headers):
        r = client.post("/budgets/", json=_budget_payload(), headers={**auth_headers, **BOOK_A})
        budget_id = r.json()["id"]
        r = client.get(f"/budgets/{budget_id}", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404

    def test_forecast_hidden_from_other_book(self, auth_headers):
        r = client.post("/forecasts/baseline", headers={**auth_headers, **BOOK_A})
        forecast_id = r.json()["id"]
        r = client.get(f"/forecasts/{forecast_id}", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404

    def test_budget_lists_scoped_to_book(self, auth_headers):
        client.post("/budgets/", json=_budget_payload("Budget A"), headers={**auth_headers, **BOOK_A})
        client.post("/budgets/", json=_budget_payload("Budget B"), headers={**auth_headers, **BOOK_B})

        r_a = client.get("/budgets/", headers={**auth_headers, **BOOK_A})
        assert [b["name"] for b in r_a.json()] == ["Budget A"]
        r_b = client.get("/budgets/", headers={**auth_headers, **BOOK_B})
        assert [b["name"] for b in r_b.json()] == ["Budget B"]

    def test_forecast_lists_scoped_to_book(self, auth_headers):
        client.post("/forecasts/baseline", headers={**auth_headers, **BOOK_A})
        client.post("/forecasts/baseline", headers={**auth_headers, **BOOK_B})

        r_a = client.get("/forecasts/", headers={**auth_headers, **BOOK_A})
        assert len(r_a.json()) == 1
        r_b = client.get("/forecasts/", headers={**auth_headers, **BOOK_B})
        assert len(r_b.json()) == 1

    def test_cross_book_update_blocked(self, auth_headers):
        r = client.post("/budgets/", json=_budget_payload(), headers={**auth_headers, **BOOK_A})
        budget_id = r.json()["id"]
        r = client.put(f"/budgets/{budget_id}", json={"name": "Hijack"}, headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404

    def test_cross_book_delete_blocked(self, auth_headers, fake_session):
        r = client.post("/budgets/", json=_budget_payload(), headers={**auth_headers, **BOOK_A})
        budget_id = r.json()["id"]
        r = client.delete(f"/budgets/{budget_id}", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404
        assert len(fake_session.budgets) == 1  # still there

    def test_cross_book_scenario_blocked(self, auth_headers):
        r = client.post("/forecasts/baseline", headers={**auth_headers, **BOOK_A})
        forecast_id = r.json()["id"]
        r = client.post(
            f"/forecasts/{forecast_id}/scenario",
            json={"name": "Stolen scenario", "revenue_growth_rate": 0.5},
            headers={**auth_headers, **BOOK_B},
        )
        assert r.status_code == 404

    def test_unscoped_request_sees_all(self, auth_headers):
        """Template semantics: no X-Book-ID header = backwards-compatible unscoped view."""
        client.post("/budgets/", json=_budget_payload("Personal"), headers=auth_headers)
        client.post("/budgets/", json=_budget_payload("Book B"), headers={**auth_headers, **BOOK_B})

        r = client.get("/budgets/", headers=auth_headers)
        assert len(r.json()) == 2  # unscoped sees personal + Book rows
