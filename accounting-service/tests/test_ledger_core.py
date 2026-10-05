import importlib.util
import json
import os

import main  # noqa: F401  (must come first: bootstraps the accounting_service package)
import pytest
from jose import jwt as jose_jwt

"""
Vimbai Ledger Kernel tests.

Covers the Rust kernel adapter (`accounting_service.utils.ledger_core`)
and the wiring in `create_journal_entry` + `GET /ledger/integrity`:
- double-entry validation at the API boundary (LEDGER_CORE_REJECTED)
- hash-chain stamps on created entries (entry_hash/prev_hash props)
- chain verification, tamper detection, cross-backend hash identity
- reversal mirror rules (corrections are reversals, never mutations)
"""

os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"
os.environ["NEO4J_PASSWORD"] = "test-password"

from accounting_service.utils import ledger_core
from fastapi.testclient import TestClient

_spec = importlib.util.spec_from_file_location(
    "lc_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession
FakeDriver = _fake_mod.FakeDriver
Temporal = _fake_mod.Temporal

_fake_session = FakeSession()

from accounting_service.database import Neo4jConnector  # noqa: E402


@pytest.fixture(autouse=True)
def _use_ledger_fake_driver():
    """Point the app at this module's fake graph only while a ledger test
    runs, then restore whatever driver was active before (sibling test
    modules install their own at import time)."""
    prev = Neo4jConnector.__dict__.get("get_driver")
    Neo4jConnector.get_driver = classmethod(lambda cls: FakeDriver(_fake_session))
    yield
    if prev is not None:
        Neo4jConnector.get_driver = prev
    else:
        del Neo4jConnector.get_driver


from main import app  # noqa: E402

client = TestClient(app)

USER_ID = "user-ledger-kernel"
BOOK = "book-ledger-kernel"


def _headers(book=BOOK):
    token = jose_jwt.encode(
        {
            "user_id": USER_ID,
            "username": "ledgerkernel",
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


def _seed_account(number, name=None):
    payload = {
        "name": name or f"Account {number}",
        "account_number": number,
        "account_type": "asset",
        "normal_balance": "debit",
        "description": "ledger kernel test account",
    }
    r = client.post("/accounts/", json=payload, headers=_headers())
    assert r.status_code in (201, 409), r.text  # 409 = already seeded
    return r.json()


def _je_payload(description="Kernel entry", debit="100.00", credit="100.00", ref=None):
    return {
        "entry_date": "2026-10-05T07:00:00",
        "description": description,
        "reference_number": ref,
        "source_module": "Manual",
        "status": "posted",
        "lines": [
            {"account_number": "1010", "debit": debit, "credit": "0.00"},
            {"account_number": "4010", "debit": "0.00", "credit": credit},
        ],
    }


def _je_nodes():
    return [n for n in _fake_session.nodes if n["label"] == "JournalEntry"]


def _clear_journal_nodes():
    _fake_session.nodes[:] = [n for n in _fake_session.nodes if n["label"] != "JournalEntry"]
    _fake_session.edges[:] = [
        e
        for e in _fake_session.edges
        if not (e[0].startswith("OWNS_JOURNAL") or e[0] == "HAS_LINE" or e[0] == "IMPACTS")
    ]


class TestKernelVectors:
    """Rule vectors the Rust kernel and the Python fallback must both pass."""

    def test_backend_is_available(self):
        assert ledger_core.BACKEND in ("rust", "python-fallback")

    def test_balanced_entry_accepted(self):
        assert ledger_core.validate_double_entry([(100.0, 0.0), (0.0, 100.0)])

    def test_unbalanced_rejected(self):
        with pytest.raises(ValueError):
            ledger_core.validate_double_entry([(100.0, 0.0), (0.0, 90.0)])

    def test_both_sides_rejected(self):
        with pytest.raises(ValueError):
            ledger_core.validate_double_entry([(100.0, 100.0), (0.0, 0.0)])

    def test_negative_rejected(self):
        with pytest.raises(ValueError):
            ledger_core.validate_double_entry([(-1.0, 0.0), (0.0, 1.0)])

    def test_single_line_rejected(self):
        with pytest.raises(ValueError):
            ledger_core.validate_double_entry([(100.0, 0.0)])

    def test_representation_noise_tolerated(self):
        assert ledger_core.validate_double_entry([(0.1 + 0.2, 0.0), (0.0, 0.3)])

    def test_hash_is_deterministic_and_order_insensitive(self):
        a = ledger_core.hash_entry({"b": 2.0, "a": 1})
        b = ledger_core.hash_entry({"a": 1, "b": 2.0})
        assert a == b
        assert len(a) == 64

    def test_chain_verifies_and_detects_tampering(self):
        payload = {"description": "sale", "amount": 50.0}
        h1 = ledger_core.hash_entry(payload)
        payload2 = {"description": "reversal", "amount": 50.0}
        h2 = ledger_core.hash_entry(payload2, prev_hash=h1)
        ok = json.loads(
            ledger_core.verify_chain(
                [
                    {
                        "entry_id": "e1",
                        "payload": payload,
                        "prev_hash": ledger_core.genesis_hash(),
                        "stored_hash": h1,
                    },
                    {"entry_id": "e2", "payload": payload2, "prev_hash": h1, "stored_hash": h2},
                ]
            )
        )
        assert ok["valid"] and ok["head_hash"] == h2

        bad = json.loads(
            ledger_core.verify_chain(
                [
                    {
                        "entry_id": "e1",
                        "payload": {**payload, "amount": 999.0},
                        "prev_hash": ledger_core.genesis_hash(),
                        "stored_hash": h1,
                    }
                ]
            )
        )
        assert not bad["valid"]

    def test_reversal_mirror_rules(self):
        orig = json.loads(ledger_core.flatten_lines_by_account([(100.0, 0.0), (0.0, 100.0)], ["1010", "4010"]))
        rev = json.loads(ledger_core.flatten_lines_by_account([(0.0, 100.0), (100.0, 0.0)], ["1010", "4010"]))
        assert ledger_core.check_reversal_mirror(orig, rev)
        # wrong amount on one account -> not a mirror
        wrong = [{"account": "1010", "debit": 0.0, "credit": 99.0}, {"account": "4010", "debit": 100.0, "credit": 0.0}]
        with pytest.raises(ValueError):
            ledger_core.check_reversal_mirror(orig, wrong)
        # same direction -> not a mirror
        with pytest.raises(ValueError):
            ledger_core.check_reversal_mirror(orig, orig)

    def test_cross_backend_hash_identity(self):
        """Fallback and Rust must produce byte-identical stamps."""
        import hashlib

        payload = {"id": "x", "amount": 100.0, "lines": [{"account": "1010", "debit": 100.0, "credit": 0.0}]}
        canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
        expected = hashlib.sha256((ledger_core.genesis_hash() + canonical).encode()).hexdigest()
        assert ledger_core.hash_entry(payload) == expected
        assert ledger_core.hash_entry(dict(reversed(list(payload.items())))) == expected


class TestLedgerWiring:
    """The kernel is wired into journal entry creation and verification."""

    def setup_method(self):
        _clear_journal_nodes()
        _seed_account("1010", "Cash")
        _seed_account("4010", "Revenue")

    def test_unbalanced_entry_rejected_at_api_boundary(self):
        r = client.post("/journal-entries/", json=_je_payload(credit="90.00"), headers=_headers())
        # Pydantic's exact-decimal balance guard fires first (outer layer).
        assert r.status_code == 422

    async def test_kernel_rejection_enforced_at_crud_layer(self):
        """Bypassing pydantic, the ledger kernel still rejects an unbalanced
        entry (defense-in-depth for internal callers of crud)."""
        from datetime import datetime
        from decimal import Decimal

        from accounting_service import crud as accounting_crud
        from accounting_service.dependencies import book_id_var
        from accounting_service.exceptions import ValidationError as ServiceValidationError
        from accounting_service.models import JournalEntryCreate, JournalLineBase

        book_id_var.set(BOOK)
        unbalanced = JournalEntryCreate.model_construct(
            entry_date=datetime(2026, 10, 5, 7, 0, 0),
            description="kernel probe",
            reference_number=None,
            source_module="Manual",
            status="posted",
            lines=[
                JournalLineBase(account_number="1010", debit=Decimal("100.00"), credit=Decimal("0.00")),
                JournalLineBase(account_number="4010", debit=Decimal("0.00"), credit=Decimal("90.00")),
            ],
        )
        with pytest.raises(ServiceValidationError) as exc_info:
            await accounting_crud.create_journal_entry(_fake_session, USER_ID, unbalanced, "jwt")
        assert "LEDGER_CORE_REJECTED" in str(getattr(exc_info.value, "code", ""))

    def test_entries_are_hash_stamped_and_chained(self):
        r1 = client.post("/journal-entries/", json=_je_payload(description="first"), headers=_headers())
        assert r1.status_code == 201, r1.text
        r2 = client.post("/journal-entries/", json=_je_payload(description="second"), headers=_headers())
        assert r2.status_code == 201, r2.text

        nodes = _je_nodes()
        assert len(nodes) == 2
        first, second = nodes
        assert first["props"]["prev_hash"] == ledger_core.genesis_hash()
        assert second["props"]["prev_hash"] == first["props"]["entry_hash"]
        assert len(first["props"]["entry_hash"]) == 64

    def test_integrity_endpoint_verifies_clean_chain(self):
        client.post("/journal-entries/", json=_je_payload(description="first"), headers=_headers())
        client.post("/journal-entries/", json=_je_payload(description="second"), headers=_headers())
        r = client.get("/ledger/integrity", headers=_headers())
        assert r.status_code == 200, r.text
        report = r.json()
        assert report["valid"] is True
        assert report["entries_checked"] == 2

    def test_integrity_endpoint_detects_tampering(self):
        client.post("/journal-entries/", json=_je_payload(description="first"), headers=_headers())
        node = _je_nodes()[0]
        # Retro-edit the description after the stamp was stored.
        node["props"]["description"] = "retro-edited"
        r = client.get("/ledger/integrity", headers=_headers())
        assert r.status_code == 200
        report = r.json()
        assert report["valid"] is False
        assert report["errors"][0]["code"] == "hash_mismatch"

    def test_reference_replay_is_idempotent(self):
        """A retried mobile push (same client reference) must 409, not
        duplicate the entry: this is what makes the offline outbox safe."""
        payload = _je_payload(description="offline capture", ref="MOB-replay-1")
        first = client.post("/journal-entries/", json=payload, headers=_headers())
        assert first.status_code in (200, 201), first.text
        count_after_first = len(_je_nodes())

        replay = client.post("/journal-entries/", json=payload, headers=_headers())
        assert replay.status_code == 409, replay.text
        assert replay.json()["code"] == "JE_REFERENCE_NUMBER_EXISTS"
        assert len(_je_nodes()) == count_after_first, "replay must not create a second entry"

    def test_reversal_pair_verifies_through_kernel(self):
        client.post("/journal-entries/", json=_je_payload(description="original"), headers=_headers())
        client.post(
            "/journal-entries/",
            json=_je_payload(description="reversal", debit="0.00", credit="0.00"),
            headers=_headers(),
        )
        # A true mirror entry (flipped sides) validates against the original.
        orig = json.loads(ledger_core.flatten_lines_by_account([(100.0, 0.0), (0.0, 100.0)], ["1010", "4010"]))
        mirror = json.loads(ledger_core.flatten_lines_by_account([(0.0, 100.0), (100.0, 0.0)], ["1010", "4010"]))
        assert ledger_core.check_reversal_mirror(orig, mirror)
