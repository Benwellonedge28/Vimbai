"""End-to-end tests for the Vimbai book-sync-service.

Runs against the FastAPI app in-process with a throwaway SQLite file.
Covers the shared-Book lifecycle: create, invite, register device,
push/pull delta sync (with retry idempotency), LWW metadata sync,
role enforcement and audit logging.
"""

import os
import tempfile

import pytest
from fastapi.testclient import TestClient

TEST_DB = os.path.join(tempfile.mkdtemp(prefix="booksync_"), "test.db")
os.environ["BOOK_SYNC_DB"] = TEST_DB

from main import app  # noqa: E402

client = TestClient(app)

ALICE = "user-alice"
BOB = "user-bob"
CAROL = "user-carol"


def hdr(user):
    return {"X-User-ID": user}


@pytest.fixture(scope="module")
def household_book():
    r = client.post(
        "/books",
        json={"name": "Mukomana Household", "tier": "household"},
        headers=hdr(ALICE),
    )
    assert r.status_code == 200, r.text
    return r.json()["book"]["id"]


def test_health():
    r = client.get("/health")
    assert r.status_code == 200
    assert r.json()["status"] == "healthy"


def test_create_book_rejects_bad_tier():
    r = client.post(
        "/books",
        json={"name": "X", "tier": "empire"},
        headers=hdr(ALICE),
    )
    assert r.status_code == 400


def test_creator_is_owner_and_sees_book(household_book):
    r = client.get("/books", headers=hdr(ALICE))
    assert r.status_code == 200
    books = r.json()["books"]
    mine = [b for b in books if b["id"] == household_book]
    assert len(mine) == 1
    assert mine[0]["your_role"] == "owner"


def test_outsider_cannot_see_book(household_book):
    r = client.get("/books/%s" % household_book, headers=hdr(BOB))
    assert r.status_code == 403


def test_invite_requires_wrapped_key(household_book):
    r = client.post(
        "/books/%s/members" % household_book,
        json={"user_id": BOB, "role": "bookkeeper"},
        headers=hdr(ALICE),
    )
    assert r.status_code == 400


def test_invite_member(household_book):
    r = client.post(
        "/books/%s/members" % household_book,
        json={
            "user_id": BOB,
            "role": "bookkeeper",
            "wrapped_book_key": "wrapped-for-bob",
        },
        headers=hdr(ALICE),
    )
    assert r.status_code == 200, r.text
    url = "/books/%s/members" % household_book
    members = client.get(url, headers=hdr(ALICE)).json()["members"]
    assert any(m["user_id"] == BOB for m in members)

    acc = client.post(
        "/books/%s/members/accept" % household_book,
        headers=hdr(BOB),
    )
    assert acc.status_code == 200, acc.text
    assert acc.json()["wrapped_book_key"] == "wrapped-for-bob"


def test_device_registration_and_push_pull(household_book):
    url = "/books/%s/devices" % household_book
    dev = client.post(url, json={"device_name": "Pixel"}, headers=hdr(BOB)).json()["device_id"]

    push = client.post(
        "/books/%s/entries/push" % household_book,
        json={
            "device_id": dev,
            "entries": [
                {"entry_id": "e1", "payload": "cipherblob1"},
                {"entry_id": "e2", "payload": "cipherblob2"},
            ],
        },
        headers=hdr(BOB),
    )
    assert push.status_code == 200, push.text
    assert push.json()["accepted"] == 2

    # Retry the same batch: must be idempotent
    retry = client.post(
        "/books/%s/entries/push" % household_book,
        json={
            "device_id": dev,
            "entries": [{"entry_id": "e1", "payload": "cipherblob1"}],
        },
        headers=hdr(BOB),
    )
    assert retry.json()["accepted"] == 0

    # Alice (owner) pulls from zero and sees both entries in order
    pulled = client.get(
        "/books/%s/entries?since=0" % household_book,
        headers=hdr(ALICE),
    )
    entries = pulled.json()["entries"]
    ids = [e["entry_id"] for e in entries]
    assert ids == ["e1", "e2"]
    assert all(e["user_id"] == BOB for e in entries)

    # Delta pull: nothing new after seq 2
    url2 = "/books/%s/entries?since=2" % household_book
    empty = client.get(url2, headers=hdr(ALICE)).json()
    assert empty["entries"] == []


def test_viewer_cannot_push(household_book):
    client.post(
        "/books/%s/members" % household_book,
        json={
            "user_id": CAROL,
            "role": "viewer",
            "wrapped_book_key": "wrapped-for-carol",
        },
        headers=hdr(ALICE),
    )
    client.post(
        "/books/%s/members/accept" % household_book,
        headers=hdr(CAROL),
    )
    dev = client.post(
        "/books/%s/devices" % household_book,
        json={},
        headers=hdr(CAROL),
    ).json()["device_id"]
    r = client.post(
        "/books/%s/entries/push" % household_book,
        json={
            "device_id": dev,
            "entries": [{"entry_id": "e3", "payload": "x"}],
        },
        headers=hdr(CAROL),
    )
    assert r.status_code == 403


def test_lww_metadata_sync(household_book):
    now = 1000.0
    client.post(
        "/books/%s/metadata" % household_book,
        json={
            "key": "book.name",
            "value": "enc-name-1",
            "updated_at": now,
        },
        headers=hdr(BOB),
    )
    client.post(
        "/books/%s/metadata" % household_book,
        json={
            "key": "book.name",
            "value": "enc-name-2",
            "updated_at": now + 10,
        },
        headers=hdr(ALICE),
    )
    rows = client.get(
        "/books/%s/metadata?since=0" % household_book,
        headers=hdr(CAROL),
    ).json()["metadata"]
    name_row = [m for m in rows if m["key"] == "book.name"][0]
    assert name_row["value"] == "enc-name-2"  # last write wins


def test_revocation_blocks_access(household_book):
    client.put(
        "/books/%s/members/%s" % (household_book, CAROL),
        json={"status": "revoked"},
        headers=hdr(ALICE),
    )
    r1 = client.get("/books/%s/entries" % household_book, headers=hdr(CAROL))
    assert r1.status_code == 403
    r2 = client.get("/books/%s" % household_book, headers=hdr(CAROL))
    assert r2.status_code == 403


def test_owner_cannot_be_modified(household_book):
    r = client.put(
        "/books/%s/members/%s" % (household_book, ALICE),
        json={"status": "revoked"},
        headers=hdr(ALICE),
    )
    assert r.status_code == 400


def test_audit_log_records_actions(household_book):
    audit = client.get("/books/%s/audit" % household_book, headers=hdr(ALICE)).json()["audit"]
    actions = {a["action"] for a in audit}
    expected = {
        "book.created",
        "member.invited",
        "entries.pushed",
        "member.updated",
    }
    assert expected <= actions


def test_nonprofit_book_tier(household_book):
    r = client.post(
        "/books",
        json={
            "name": "Tariro Community Trust",
            "tier": "nonprofit",
            "wrapped_book_key": "wrapped-for-npo",
        },
        headers=hdr(ALICE),
    )
    assert r.status_code == 200
    assert r.json()["book"]["tier"] == "nonprofit"


def test_personal_book_needs_no_key(household_book):
    r = client.post(
        "/books",
        json={"name": "My Personal", "tier": "personal"},
        headers=hdr(BOB),
    )
    assert r.status_code == 200
    assert r.json()["book"]["tier"] == "personal"


def test_membership_check_endpoint(household_book):
    # invited member can check their membership and sees their role
    client.post(
        "/books/%s/members" % household_book,
        json={"user_id": "member-two", "role": "bookkeeper", "wrapped_book_key": "aW52aXRlLXdyYXB="},
        headers=hdr(ALICE),
    )
    client.post(
        "/books/%s/members/accept" % household_book,
        json={"user_id": "member-two"},
        headers=hdr("member-two"),
    )
    r = client.get("/books/%s/membership" % household_book, headers=hdr("member-two"))
    assert r.status_code == 200, r.text
    j = r.json()
    assert j["role"] == "bookkeeper"
    assert j["tier"] == "household"
    # a non-member gets 403
    r = client.get("/books/%s/membership" % household_book, headers=hdr("intruder"))
    assert r.status_code == 403


def test_default_book_get_or_create():
    hdrs = hdr("fresh-user")
    r1 = client.post("/books/default", headers=hdrs)
    assert r1.status_code == 200, r1.text
    first = r1.json()
    assert first["created"] is True
    assert first["book"]["tier"] == "personal"
    assert first["book"]["name"] == "My Book"
    # second call returns the same book, no duplicate
    r2 = client.post("/books/default", headers=hdrs)
    assert r2.json()["created"] is False
    assert r2.json()["book"]["id"] == first["book"]["id"]
