"""Book-scoping and IFRS reporting tests for ifrs-reporting-service."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from ifrs_reporting_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ifrs_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ifrs-user-1", "ifrs-user-2"
BOOK_A, BOOK_B = "ifrs-book-a", "ifrs-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
H1_BOOK_B = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _create(report_type="balance_sheet", standard="IAS1", status=None, headers=H1, figure=1000.0):
    return client.post(
        "/reports",
        params={
            "report_type": report_type,
            "ifrs_standard": standard,
            "period": "2026-10",
            "reporting_date": "2026-10-31T00:00:00",
        },
        json={"total_assets": figure},
        headers=headers,
    )


class TestCreate:
    def test_a_report_keeps_its_figures(self):
        body = _create().json()
        assert body["data"] == {"total_assets": 1000.0}
        assert body["status"] == "draft"
        assert body["disclosures"] == []
        assert body["book_id"] == BOOK_A

    def test_an_unknown_report_type_is_rejected_with_the_list(self):
        response = _create(report_type="trial_balance")
        assert response.status_code == 400
        detail = response.json()["detail"]
        assert detail.startswith("Invalid report type")
        assert "equity_changes" in detail

    def test_the_listing_filters_by_standard_and_status(self):
        _create(standard="IAS1")
        _create(standard="IFRS16", report_type="notes")
        assert len(client.get("/reports", headers=H1).json()) == 2
        by_standard = client.get("/reports", params={"ifrs_standard": "IFRS16"}, headers=H1).json()
        assert [r["report_type"] for r in by_standard] == ["notes"]
        assert client.get("/reports", params={"status": "approved"}, headers=H1).json() == []

    def test_reports_are_book_scoped(self):
        report = _create().json()
        assert client.get("/reports", headers=H1_BOOK_B).json() == []
        assert client.get(f"/reports/{report['id']}", headers=H1_BOOK_B).status_code == 404


class TestRead:
    def test_a_missing_report_is_a_404(self):
        response = client.get("/reports/ghost", headers=H1)
        assert response.status_code == 404
        assert response.json()["detail"] == "Report not found"

    def test_another_users_report_is_a_404(self):
        report = _create().json()
        assert client.get(f"/reports/{report['id']}", headers=H2).status_code == 404


class TestNotes:
    def test_a_note_also_lands_in_the_report_disclosure_index(self):
        report = _create().json()
        note = client.post(
            f"/reports/{report['id']}/notes",
            params={"note_number": 1, "title": "Basis of preparation", "ifrs_reference": "IAS1.112"},
            headers=H1,
        ).json()
        assert note["note_number"] == 1
        stored = client.get(f"/reports/{report['id']}", headers=H1).json()
        assert stored["disclosures"] == [{"note_number": 1, "title": "Basis of preparation"}]

    def test_notes_are_listed_for_their_report(self):
        first = _create().json()
        second = _create(report_type="cash_flow").json()
        client.post(
            f"/reports/{first['id']}/notes",
            params={"note_number": 1, "title": "A"},
            headers=H1,
        )
        client.post(
            f"/reports/{second['id']}/notes",
            params={"note_number": 2, "title": "B"},
            headers=H1,
        )
        listed = client.get(f"/reports/{first['id']}/notes", headers=H1).json()
        assert [n["title"] for n in listed] == ["A"]

    def test_notes_for_an_unknown_report_are_an_empty_list(self):
        """The original answered an empty list rather than a not-found here."""
        response = client.get("/reports/ghost/notes", headers=H1)
        assert response.status_code == 200
        assert response.json() == []

    def test_a_note_cannot_be_added_to_another_users_report(self):
        report = _create().json()
        response = client.post(
            f"/reports/{report['id']}/notes",
            params={"note_number": 1, "title": "A"},
            headers=H2,
        )
        assert response.status_code == 404

    def test_another_users_notes_stay_hidden(self):
        report = _create().json()
        client.post(
            f"/reports/{report['id']}/notes",
            params={"note_number": 1, "title": "A"},
            headers=H1,
        )
        assert client.get(f"/reports/{report['id']}/notes", headers=H2).json() == []


class TestApproval:
    def test_a_draft_can_be_approved(self):
        report = _create().json()
        body = client.put(f"/reports/{report['id']}/approve", params={"approved_by": "cfo"}, headers=H1).json()
        assert body == {"report_id": report["id"], "status": "approved", "approved_by": "cfo"}
        stored = client.get(f"/reports/{report['id']}", headers=H1).json()
        assert stored["status"] == "approved"
        assert stored["approved_by"] == "cfo"

    def test_an_approved_report_cannot_be_approved_again(self):
        report = _create().json()
        client.put(f"/reports/{report['id']}/approve", params={"approved_by": "cfo"}, headers=H1)
        response = client.put(f"/reports/{report['id']}/approve", params={"approved_by": "chair"}, headers=H1)
        assert response.status_code == 400
        assert "cannot be approved from approved state" in response.json()["detail"]

    def test_another_user_cannot_approve_it(self):
        report = _create().json()
        response = client.put(f"/reports/{report['id']}/approve", params={"approved_by": "cfo"}, headers=H2)
        assert response.status_code == 404
