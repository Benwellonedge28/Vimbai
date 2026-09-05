"""Book-scoped persistence verification for accounting-service.

Verifies the post-rollout contract on a fake Neo4j graph:
- records created without X-Book-ID (personal scope, book_id NULL) persist
- legacy records that predate the rollout (no book_id property at all) persist
- Book-scoped records stay isolated across Books and survive cross-traffic
- incomplete/unstamped records never leak into a Book's queries (404/absent)
"""

import importlib.util
import os
from datetime import datetime, timezone

# Must precede package imports: main.py bootstraps the accounting_service package.
import main  # noqa: F401
from accounting_service.database import Neo4jConnector
from jose import jwt as jose_jwt

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")
from fastapi.testclient import TestClient  # noqa: E402
from main import app  # noqa: E402

client = TestClient(app)  # no context manager: startup (real Neo4j) never runs

_spec = importlib.util.spec_from_file_location(
    "acct_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession
FakeDriver = _fake_mod.FakeDriver
Temporal = _fake_mod.Temporal

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: FakeDriver(_fake_session))

USER_ID = "user-book-verify"
BOOK_A = "book-verify-a"
BOOK_B = "book-verify-b"


def _headers(book=None):
    token = jose_jwt.encode(
        {
            "user_id": USER_ID,
            "username": "bookverifier",
            "role": "SUPER_ADMIN",
            "permissions": ["*.*"],
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    h = {"Authorization": f"Bearer {token}"}
    if book:
        h["X-Book-ID"] = book
    return h


def _account_payload(number, name=None):
    return {
        "name": name or f"Account {number}",
        "account_number": number,
        "account_type": "asset",
        "normal_balance": "debit",
        "description": "book persistence verification",
    }


def _seed_legacy_account(number):
    """Simulate a record written before the Book rollout: no book_id property."""
    _fake_session.nodes.append(
        {
            "label": "Account",
            "props": {
                "id": f"legacy-{number}",
                "book_id": None,
                "name": f"Legacy Account {number}",
                "account_number": number,
                "account_type": "asset",
                "normal_balance": "debit",
                "description": "pre-rollout record",
                "parent_account_number": None,
                "created_at": Temporal(datetime.now(timezone.utc).isoformat()),
                "updated_at": Temporal(datetime.now(timezone.utc).isoformat()),
                "user_id": USER_ID,
            },
        }
    )
    # attach the ownership edge to the seeded node
    node = _fake_session.nodes[-1]
    _fake_session.edges.append(("OWNS_ACCOUNT", USER_ID, node))


def _get_node(account_number):
    return next(
        (n for n in _fake_session.nodes if n["label"] == "Account" and n["props"]["account_number"] == account_number),
        None,
    )


class TestIncompleteRecordsPersistence:
    def test_personal_scope_record_persists_without_book(self):
        r = client.post("/accounts/", json=_account_payload("9001"), headers=_headers())
        assert r.status_code == 201, r.text
        assert r.json()["account_number"] == "9001"

        listed = client.get("/accounts/", headers=_headers())
        assert any(a["account_number"] == "9001" for a in listed.json())

        node = _get_node("9001")
        assert node is not None, "personal-scope record must persist"
        assert node["props"]["book_id"] is None

    def test_unstamped_record_never_leaks_into_book_scope(self):
        r = client.post("/accounts/", json=_account_payload("9002"), headers=_headers())
        assert r.status_code == 201

        listed_a = client.get("/accounts/", headers=_headers(book=BOOK_A))
        assert all(a["account_number"] != "9002" for a in listed_a.json())

        fetched = client.get("/accounts/9002", headers=_headers(book=BOOK_A))
        assert fetched.status_code == 404

    def test_legacy_prewrite_record_still_readable_personally(self):
        _seed_legacy_account("8000")

        fetched = client.get("/accounts/8000", headers=_headers())
        assert fetched.status_code == 200
        assert fetched.json()["name"] == "Legacy Account 8000"

        listed = client.get("/accounts/", headers=_headers())
        assert any(a["account_number"] == "8000" for a in listed.json())

    def test_legacy_record_invisible_under_book_header(self):
        _seed_legacy_account("8001")

        fetched = client.get("/accounts/8001", headers=_headers(book=BOOK_B))
        assert fetched.status_code == 404

        listed = client.get("/accounts/", headers=_headers(book=BOOK_B))
        assert all(a["account_number"] != "8001" for a in listed.json())


class TestBookScopedPersistence:
    def test_book_records_isolated_and_stamped(self):
        a = client.post("/accounts/", json=_account_payload("7001"), headers=_headers(book=BOOK_A))
        assert a.status_code == 201, a.text
        b = client.post("/accounts/", json=_account_payload("7002"), headers=_headers(book=BOOK_B))
        assert b.status_code == 201

        assert _get_node("7001")["props"]["book_id"] == BOOK_A
        assert _get_node("7002")["props"]["book_id"] == BOOK_B

        listed_a = client.get("/accounts/", headers=_headers(book=BOOK_A))
        nums_a = {x["account_number"] for x in listed_a.json()}
        assert "7001" in nums_a and "7002" not in nums_a

    def test_records_survive_cross_book_traffic(self):
        for i in range(3):
            r = client.post("/accounts/", json=_account_payload(f"710{i}"), headers=_headers(book=BOOK_A))
            assert r.status_code == 201

        # heavy traffic from Book B must not disturb Book A's records
        for i in range(3):
            client.post("/accounts/", json=_account_payload(f"720{i}"), headers=_headers(book=BOOK_B))

        listed_a = client.get("/accounts/", headers=_headers(book=BOOK_A))
        nums_a = {x["account_number"] for x in listed_a.json()}
        assert {"7100", "7101", "7102"} <= nums_a
        assert not ({"7200", "7201", "7202"} & nums_a)

    def test_update_blocked_cross_book(self):
        created = client.post("/accounts/", json=_account_payload("7300"), headers=_headers(book=BOOK_A))
        assert created.status_code == 201

        upd = client.put(
            "/accounts/7300", json={"description": "sneaky cross-book edit"}, headers=_headers(book=BOOK_B)
        )
        assert upd.status_code == 404

        upd_same = client.put(
            "/accounts/7300", json={"description": "legit same-book edit"}, headers=_headers(book=BOOK_A)
        )
        assert upd_same.status_code == 200
        assert _get_node("7300")["props"]["description"] == "legit same-book edit"

    def test_update_personal_record_from_personal_scope(self):
        client.post("/accounts/", json=_account_payload("7400"), headers=_headers())
        upd = client.put("/accounts/7400", json={"description": "personal edit"}, headers=_headers())
        assert upd.status_code == 200
        assert _get_node("7400")["props"]["description"] == "personal edit"
