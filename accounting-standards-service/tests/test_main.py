"""Book-scoping and persistence tests for accounting-standards-service (fake Neo4j harness).

Covers: configuration upsert/persist/activate/deactivate, account mappings,
policy upsert per area, compliance check history replacement, ownership and
Book isolation, cross-scope 404s.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from accounting_standards_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("astd_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "astd-user-1", "astd-user-2"
BOOK_A, BOOK_B = "astd-book-a", "astd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
ORG = "astd-org-1"

CONFIG = {
    "id": "",
    "organization_id": ORG,
    "selected_standards": ["ifrs"],
    "measurement_base": "fair_value",
    "disclosure_level": "standard",
    "functional_currency": "USD",
    "fiscal_year_end": "December",
}


def _create_config(headers=H1, org=ORG, standards=("ifrs",)):
    payload = {**CONFIG, "organization_id": org, "selected_standards": list(standards)}
    r = client.post(f"/organizations/{org}/configuration", json=payload, headers=headers)
    assert r.status_code == 200, r.text
    return r.json()


def test_configuration_upsert_and_persistence():
    c1 = _create_config(standards=("ifrs",))
    c2 = _create_config(standards=("ifrs", "us_gaap"))

    # upsert: only one config per caller+org, and it persists with the new value
    got = client.get(f"/organizations/{ORG}/configuration", headers=H1).json()
    assert got["id"] == c2["id"]
    assert got["id"] != c1["id"]
    assert set(got["selected_standards"]) == {"ifrs", "us_gaap"}


def test_configuration_cross_user_404():
    _create_config()
    assert client.get(f"/organizations/{ORG}/configuration", headers=H2).status_code == 404

    r = client.put(
        f"/organizations/{ORG}/configuration",
        json={**CONFIG, "functional_currency": "EUR"},
        headers=H2,
    )
    assert r.status_code == 404


def test_configuration_update_keeps_identity():
    created = _create_config()
    r = client.put(
        f"/organizations/{ORG}/configuration",
        json={**CONFIG, "functional_currency": "ZWL"},
        headers=H1,
    )
    assert r.status_code == 200
    updated = r.json()
    assert updated["id"] == created["id"]
    assert updated["functional_currency"] == "ZWL"

    got = client.get(f"/organizations/{ORG}/configuration", headers=H1).json()
    assert got["functional_currency"] == "ZWL"


def test_book_isolation():
    _create_config(headers=H1, org="bookA-org")
    _create_config(headers=HB, org="bookB-org")

    assert client.get("/organizations/bookA-org/configuration", headers=H1).status_code == 200
    assert client.get("/organizations/bookA-org/configuration", headers=HB).status_code == 404
    assert client.get("/organizations/bookB-org/configuration", headers=HB).status_code == 200


def test_activate_deactivate_standard_persists():
    _create_config(standards=("ifrs",))

    r = client.post(f"/organizations/{ORG}/standards/us_gaap/activate", headers=H1)
    assert r.status_code == 200 and r.json()["status"] == "activated"
    got = client.get(f"/organizations/{ORG}/configuration", headers=H1).json()
    assert "us_gaap" in got["selected_standards"]

    # activate twice is idempotent
    client.post(f"/organizations/{ORG}/standards/us_gaap/activate", headers=H1)
    got = client.get(f"/organizations/{ORG}/configuration", headers=H1).json()
    assert got["selected_standards"].count("us_gaap") == 1

    r = client.post(f"/organizations/{ORG}/standards/us_gaap/deactivate", headers=H1)
    assert r.status_code == 200 and r.json()["status"] == "deactivated"
    got = client.get(f"/organizations/{ORG}/configuration", headers=H1).json()
    assert "us_gaap" not in got["selected_standards"]

    # cross-scope activate 404s
    assert client.post(f"/organizations/{ORG}/standards/us_gaap/activate", headers=H2).status_code == 404


MAPPING = {
    "local_code": "1000",
    "local_name": "Cash",
    "standard_code": "IFRS-CASH",
    "standard_name": "Cash and equivalents",
    "standard_type": "ifrs",
    "category": "asset",
    "classification": "current",
    "measurement": "fair_value",
    "is_required": True,
}


def test_account_mapping_append_and_isolation():
    r = client.post("/standards/ifrs/account-mapping", json=MAPPING, headers=H1)
    assert r.status_code == 200 and r.json()["status"] == "added"

    # only the caller's mappings are visible, keyed by path standard
    mine = client.get("/standards/ifrs/account-mapping", headers=H1).json()
    assert len(mine) == 1 and mine[0]["local_code"] == "1000"
    assert client.get("/standards/ifrs/account-mapping", headers=H2).json() == []
    assert client.get("/standards/us_gaap/account-mapping", headers=H1).json() == []


def test_policy_upsert_per_area():
    payload = {
        "id": "",
        "organization_id": ORG,
        "standard_type": "ifrs",
        "policy_area": "revenue",
        "policy_description": "5-step model",
        "selected_method": "point_in_time",
        "alternative_methods": ["over_time"],
        "justification": "Control transfer",
        "disclosure_text": "Recognized on transfer of control",
        "effective_date": "2024-01-01",
        "approved_by": "CFO",
    }
    r = client.post(f"/organizations/{ORG}/policies", json=payload, headers=H1)
    assert r.status_code == 200, r.text

    # upsert same org+standard+area
    payload["selected_method"] = "over_time"
    r2 = client.post(f"/organizations/{ORG}/policies", json=payload, headers=H1)
    assert r2.status_code == 200

    policies = client.get(f"/organizations/{ORG}/policies", headers=H1).json()
    assert len(policies) == 1
    assert policies[0]["selected_method"] == "over_time"

    # area lookup
    got = client.get(f"/organizations/{ORG}/policies/revenue", params={"standard_type": "ifrs"}, headers=H1).json()
    assert got["selected_method"] == "over_time"

    # cross-user isolation
    assert client.get(f"/organizations/{ORG}/policies", headers=H2).json() == []
    assert (
        client.get(f"/organizations/{ORG}/policies/revenue", params={"standard_type": "ifrs"}, headers=H2).status_code
        == 404
    )


def test_compliance_check_replaces_history_and_isolated():
    body = {
        "validation_results": [
            {"code": "IFRS_15", "status": "fail", "finding": "Missing contract assets", "severity": "major"},
            {"code": "IFRS_16", "status": "warning"},
        ]
    }
    r = client.post(f"/organizations/{ORG}/compliance/check", params={"standard_type": "ifrs"}, json=body, headers=H1)
    assert r.status_code == 200, r.text
    summary = r.json()
    assert summary["total_checks"] == 4
    assert summary["passed"] == 2
    assert summary["failed"] == 1
    assert summary["warnings"] == 1

    history = client.get(f"/organizations/{ORG}/compliance/history", headers=H1).json()
    assert len(history) == 4

    # rerun replaces (not appends) the caller's history
    client.post(
        f"/organizations/{ORG}/compliance/check",
        params={"standard_type": "ifrs"},
        json={"validation_results": []},
        headers=H1,
    )
    history = client.get(f"/organizations/{ORG}/compliance/history", headers=H1).json()
    assert len(history) == 4
    assert all(c["status"] == "pass" for c in history)

    # other caller sees nothing
    assert client.get(f"/organizations/{ORG}/compliance/history", headers=H2).json() == []


def test_pure_catalogue_endpoints():
    standards = client.get("/standards").json()
    assert len(standards) >= 13

    ifrs = client.get("/standards/ifrs").json()
    assert ifrs["code"] == "IFRS"

    reqs = client.get("/standards/ifrs/requirements").json()
    assert len(reqs) == 4

    comparison = client.get("/standards/comparison", params={"standard_1": "ifrs", "standard_2": "us_gaap"}).json()
    assert "comparison" in comparison
    assert comparison["comparison"]["key_principles_common"] is not None

    guide = client.get("/standards/ifrs/measurement-guide").json()
    assert guide["measurement_basis"] == "fair_value"

    assert client.get("/standards/categories").status_code == 200
