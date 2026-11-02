"""Book-scoping and persistence tests for cash-flow-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from cash_flow_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("cash_flow_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "cf-user-1", "cf-user-2"
BOOK_A, BOOK_B = "cf-book-a", "cf-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _create(body, headers=H1):
    """The scalars travel as query params, the year lists as a body, exactly as
    the original signature made FastAPI read them."""
    params = {"project_name": body["project_name"], "initial_investment": body["initial_investment"]}
    payload = {k: v for k, v in body.items() if k in ("yearly_inflows", "yearly_outflows")}
    return client.post("/create", params=params, json=payload, headers=headers)


def _minimal_params():
    return {
        "project_name": "Plant expansion",
        "initial_investment": 500000,
        "yearly_inflows": [200000, 250000, 300000],
        "yearly_outflows": [50000, 60000],
    }


class TestSchedules:
    def test_create_sums_the_years(self):
        resp = _create(_minimal_params())
        assert resp.status_code == 200, resp.text
        body = resp.json()
        assert [e["net_flow"] for e in body["entries"]] == [150000.0, 190000.0, 300000.0]
        # the third year has no outflow, so it keeps a zero
        assert body["entries"][2]["outflow"] == 0
        assert body["total_inflows"] == 750000.0
        assert body["total_outflows"] == 110000.0
        assert body["total_net_flow"] == 640000.0
        assert body["book_id"] == BOOK_A

    def test_a_schedule_survives_a_read_back(self):
        created = _create(_minimal_params()).json()
        listed = client.get("/list", headers=H1).json()
        assert [s["id"] for s in listed["cash_flows"]] == [created["id"]]
        fetched = client.get(f"/get/{created['id']}", headers=H1).json()
        assert fetched["project_name"] == "Plant expansion"
        assert len(fetched["entries"]) == 3
        assert fetched["total_net_flow"] == 640000.0

    def test_another_user_cannot_read_it(self):
        created = _create(_minimal_params()).json()
        assert client.get("/list", headers=H2).json()["cash_flows"] == []
        assert client.get(f"/get/{created['id']}", headers=H2).json() == {"error": "Cash flow not found"}

    def test_another_book_cannot_read_it(self):
        created = _create(_minimal_params()).json()
        assert client.get("/list", headers=H1_BOOK_B).json()["cash_flows"] == []
        assert client.get(f"/get/{created['id']}", headers=H1_BOOK_B).json() == {"error": "Cash flow not found"}

    def test_an_unknown_id_reads_as_not_found(self):
        assert client.get("/get/nope", headers=H1).json() == {"error": "Cash flow not found"}

    def test_schedules_list_oldest_first(self):
        first = _create(_minimal_params()).json()
        second = _create({**_minimal_params(), "project_name": "Fleet"}).json()
        listed = client.get("/list", headers=H1).json()["cash_flows"]
        assert [s["id"] for s in listed] == [first["id"], second["id"]]


class TestCalculators:
    def test_inflows_and_outflows(self):
        assert client.post("/inflows", json=[100, 250]).json()["total_inflows"] == 350
        assert client.post("/outflows", json=[50, 25]).json()["total_outflows"] == 75

    def test_net_flows_pads_the_shorter_side(self):
        body = client.post("/net-flows", json={"inflows": [10, 20, 30], "outflows": [5]}).json()
        assert body["net_flows"] == [5.0, 20.0, 30.0]
        assert body["total_net_flow"] == 55.0

    def test_cumulative_flows_run_a_total(self):
        body = client.post("/cumulative-flows", json=[10, -5, 20]).json()
        assert body["cumulative_flows"] == [10, 5, 25]
