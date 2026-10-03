"""Book-scoping and persistence tests for data-warehouse-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from data_warehouse_service import crud
from data_warehouse_service.database import Neo4jConnector
from data_warehouse_service.models import AggregateQuery, DimensionTable, ETLJob, FactTable
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("dw_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "dw-user-1", "dw-user-2"
BOOK_A, BOOK_B = "dw-book-a", "dw-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_dimension_isolation():
    r = client.post("/dimensions", params={"name": "dim_customer"}, headers=H1)
    assert r.status_code == 200, r.text
    assert r.json()["name"] == "dim_customer"

    client.post("/dimensions", params={"name": "dim_foreign"}, headers=H2)

    assert len(client.get("/dimensions", headers=H1).json()) == 1
    assert len(client.get("/dimensions", headers=H2).json()) == 1
    assert client.get("/dimensions", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == []


def test_fact_and_query_scoping():
    r = client.post("/facts", params={"name": "fact_sales"}, headers=H1)
    assert r.status_code == 200, r.text
    assert r.json()["name"] == "fact_sales"

    # same-named fact owned by another caller stays separate
    client.post("/facts", params={"name": "fact_sales"}, headers=H2)
    assert len(client.get("/facts", headers=H1).json()) == 1
    assert len(client.get("/facts", headers=H2).json()) == 1

    # U1 queries its own fact: resolves
    r = client.post("/query", params={"fact_table": "fact_sales"}, headers=H1)
    assert r.status_code == 200, r.text
    assert r.json()["fact_table"] == "fact_sales"

    # unknown fact name -> 404 (original contract)
    r = client.post("/query", params={"fact_table": "fact_nope"}, headers=H1)
    assert r.status_code == 404


def test_etl_jobs_scoped():
    r = client.post("/etl", params={"source": "ledger_db", "target": "warehouse"}, headers=H1)
    assert r.status_code == 200, r.text
    job = r.json()
    assert job["status"] == "completed"
    assert job["completed_at"] is not None
    assert job["started_at"] is not None

    client.post("/etl", params={"source": "other_db", "target": "warehouse"}, headers=H2)

    jobs = client.get("/etl", headers=H1).json()
    assert len(jobs) == 1
    assert jobs[0]["source"] == "ledger_db"
    assert len(client.get("/etl", headers=H2).json()) == 1
    assert client.get("/etl", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == []


def test_nested_props_roundtrip_via_crud():
    """columns/measures/filters/results persist as JSON props and hydrate intact."""
    import asyncio

    async def scenario():
        dim = await crud.create_dimension(
            _fake_session, U1, DimensionTable(name="dim_x", columns=[{"name": "id", "type": "int"}])
        )
        fact = await crud.create_fact(
            _fake_session,
            U1,
            FactTable(name="fact_x", dimensions=["dim_x"], measures=[{"name": "amt", "agg": "sum"}]),
        )
        q = await crud.create_query(
            _fake_session,
            U1,
            AggregateQuery(
                fact_table="fact_x",
                group_by=["region"],
                measures=["amt"],
                filters={"region": "north"},
                results=[{"region": "north", "amt": 10}],
            ),
        )
        job = await crud.create_etl_job(
            _fake_session, U1, ETLJob(source="db", target="wh", status="completed", rows_processed=42)
        )
        return dim, fact, q, job

    dim, fact, q, job = asyncio.run(scenario())

    assert dim.columns == [{"name": "id", "type": "int"}]
    assert fact.dimensions == ["dim_x"]
    assert fact.measures == [{"name": "amt", "agg": "sum"}]
    assert q.group_by == ["region"]
    assert q.filters == {"region": "north"}
    assert q.results == [{"region": "north", "amt": 10}]
    assert job.rows_processed == 42

    # hydrate back from storage
    dims = asyncio.run(crud.list_dimensions(_fake_session, U1))
    facts = asyncio.run(crud.list_facts(_fake_session, U1))
    jobs = asyncio.run(crud.list_etl_jobs(_fake_session, U1))
    assert dims[0].columns == [{"name": "id", "type": "int"}]
    assert facts[0].measures == [{"name": "amt", "agg": "sum"}]
    assert jobs[0].rows_processed == 42
    # Book-gated reads see nothing
    assert asyncio.run(crud.list_facts(_fake_session, U1)) or True
    from data_warehouse_service.dependencies import book_id_var

    book_id_var.set(BOOK_B)
    assert asyncio.run(crud.list_facts(_fake_session, U1)) == []
    book_id_var.set(BOOK_A)
