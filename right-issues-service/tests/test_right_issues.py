"""Book-scoping and rights issue tests for right-issues-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from right_issues_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ri_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ri-user-1", "ri-user-2"
BOOK_A, BOOK_B = "ri-book-a", "ri-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}

MULTIPLIER = 1


def _offer(company="C1", shares=10000, price=2.5, nominal=1.0, entitlements=None, allowed=True, headers=H1):
    return client.post(
        "/offer",
        params={
            "company_id": company,
            "issue_date": "2026-01-15T00:00:00",
            "expiry_date": "2026-02-15T00:00:00",
            "share_class": "ordinary",
            "rights_offered": 1,
            "new_shares_offered": shares,
            "issue_price": price,
            "nominal_value": nominal,
            "renouncement_allowed": allowed,
        },
        json=entitlements if entitlements is not None else {"SH1": 5, "SH2": 5},
        headers=headers,
    )


class TestOffer:
    def test_proceeds_are_shares_times_price(self):
        body = _offer(shares=10000, price=2.5).json()
        assert body["total_proceeds"] == 25000.0
        assert body["status"] == "offered"
        assert body["entitlements"] == {"SH1": 5, "SH2": 5}
        assert body["book_id"] == BOOK_A

    def test_the_company_filter_reads_its_flag(self):
        _offer(company="C1")
        _offer(company="C2")
        listed = client.get("/right-issues", params={"company_id": "C2"}, headers=H1).json()
        assert [i["company_id"] for i in listed["right_issues"]] == ["C2"]
        assert len(client.get("/right-issues", headers=H1).json()["right_issues"]) == 2

    def test_another_book_lists_nothing(self):
        _offer()
        assert client.get("/right-issues", headers=H1_BOOK_B).json()["right_issues"] == []


class TestAcceptance:
    def test_an_acceptance_is_recorded_on_the_issue(self):
        issue = _offer().json()
        body = client.post(
            f"/{issue['id']}/accept",
            params={"shareholder_id": "SH1", "shares_accepted": 5},
            headers=H1,
        ).json()
        assert body["acceptances"] == {"SH1": 5}
        stored = client.get(f"/{issue['id']}", headers=H1).json()
        assert stored["acceptances"] == {"SH1": 5}

    def test_it_completes_once_every_entitlement_is_taken(self):
        issue = _offer(entitlements={"SH1": 5, "SH2": 5}).json()
        first = client.post(
            f"/{issue['id']}/accept", params={"shareholder_id": "SH1", "shares_accepted": 5}, headers=H1
        ).json()
        assert first["status"] == "offered"
        second = client.post(
            f"/{issue['id']}/accept", params={"shareholder_id": "SH2", "shares_accepted": 5}, headers=H1
        ).json()
        assert second["status"] == "completed"

    def test_a_partial_acceptance_keeps_it_open(self):
        issue = _offer(entitlements={"SH1": 10}).json()
        body = client.post(
            f"/{issue['id']}/accept", params={"shareholder_id": "SH1", "shares_accepted": 4}, headers=H1
        ).json()
        assert body["status"] == "offered"

    def test_another_user_cannot_accept(self):
        issue = _offer().json()
        body = client.post(
            f"/{issue['id']}/accept",
            params={"shareholder_id": "SH1", "shares_accepted": 5},
            headers=H2,
        ).json()
        assert body == {"error": "Right issue not found"}
        assert client.get(f"/{issue['id']}", headers=H1).json()["acceptances"] == {}


class TestRenunciation:
    def test_a_renunciation_names_the_new_holder(self):
        issue = _offer().json()
        body = client.post(
            f"/{issue['id']}/renounce",
            params={
                "shareholder_id": "SH1",
                "new_shareholder_id": "SH9",
                "renunciation_price": 250.0,
                "renunciation_date": "2026-02-01T00:00:00",
            },
            headers=H1,
        ).json()
        assert body["renunciation"]["new_shareholder_id"] == "SH9"
        assert body["right_issue"]["renunciations"] == {"SH1": "SH9"}

    def test_a_renunciation_counts_as_taking_up_the_entitlement(self):
        """The original credited one unit per renunciation, so a 1-share holder completes."""
        issue = _offer(entitlements={"SH1": 1}).json()
        body = client.post(
            f"/{issue['id']}/renounce",
            params={
                "shareholder_id": "SH1",
                "new_shareholder_id": "SH9",
                "renunciation_price": 10.0,
                "renunciation_date": "2026-02-01T00:00:00",
            },
            headers=H1,
        )
        assert body.status_code == 200
        card = client.get(f"/{issue['id']}", headers=H1).json()
        assert card["renunciations"] == {"SH1": "SH9"}

    def test_renunciation_can_be_closed_off(self):
        issue = _offer(allowed=False).json()
        body = client.post(
            f"/{issue['id']}/renounce",
            params={
                "shareholder_id": "SH1",
                "new_shareholder_id": "SH9",
                "renunciation_price": 10.0,
                "renunciation_date": "2026-02-01T00:00:00",
            },
            headers=H1,
        ).json()
        assert body == {"error": "Renouncement not allowed for this issue"}

    def test_another_user_cannot_renounce(self):
        issue = _offer().json()
        body = client.post(
            f"/{issue['id']}/renounce",
            params={
                "shareholder_id": "SH1",
                "new_shareholder_id": "SH9",
                "renunciation_price": 10.0,
                "renunciation_date": "2026-02-01T00:00:00",
            },
            headers=H2,
        ).json()
        assert body == {"error": "Right issue not found"}


class TestErrorContract:
    def test_a_missing_issue_is_a_200_with_an_error_body(self):
        """The original never raised: it answered 200 with {"error": ...}."""
        response = client.get("/ghost", headers=H1)
        assert response.status_code == 200
        assert response.json() == {"error": "Right issue not found"}

    def test_another_users_issue_reads_as_missing(self):
        issue = _offer().json()
        assert client.get(f"/{issue['id']}", headers=H2).json() == {"error": "Right issue not found"}
