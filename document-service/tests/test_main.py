"""Document Service tests — Neo4j-backed metadata with Book scoping (fake harness)."""

import importlib.util
import os

import main  # noqa: F401  (must come first: bootstraps the document_service package)
from document_service.database import Neo4jConnector
from fastapi.testclient import TestClient  # noqa: E402

client = TestClient(app := main.app)

_spec = importlib.util.spec_from_file_location(
    "document_fake_neo4j", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fake_neo4j.py")
)
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

USER = "user-doc"
OTHER_USER = "user-other"
BOOK_A = "book-doc-a"
BOOK_B = "book-doc-b"

PDF_BYTES = b"%PDF-1.4 fake invoice content"
PNG_BYTES = b"\x89PNG\r\n\x1a\n fake image"


def _headers(user=USER, book=None):
    h = {"X-User-Id": user}
    if book:
        h["X-Book-ID"] = book
    return h


def _upload(
    title="Invoice Q3",
    fname="invoice.pdf",
    content=PDF_BYTES,
    doc_type="invoice",
    tags=None,
    headers=None,
    user=USER,
    book=None,
):
    return client.post(
        "/documents",
        files={"file": (fname, content, "application/octet-stream")},
        data={
            "title": title,
            "document_type": doc_type,
            "tags": tags or "finance, q3",
        },
        headers=headers or _headers(user=user, book=book),
    )


def _clear():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


class TestUpload:
    def setup_method(self):
        _clear()

    def test_upload_document(self):
        r = _upload()
        assert r.status_code == 201, r.text
        data = r.json()
        assert data["title"] == "Invoice Q3"
        assert data["status"] == "uploaded"
        assert data["tags"] == ["finance", "q3"]
        assert data["mime_type"] == "application/pdf"
        assert data["file_size"] == len(PDF_BYTES)
        assert data["user_id"] == USER

    def test_upload_stamps_book(self):
        r = _upload(title="Book doc", book=BOOK_A)
        assert r.status_code == 201
        assert r.json()["book_id"] == BOOK_A

    def test_upload_rejects_bad_extension(self):
        r = _upload(fname="malware.exe", content=b"MZ")
        assert r.status_code == 400

    def test_upload_rejects_duplicate_content(self):
        assert _upload().status_code == 201
        r = _upload(title="Duplicate")
        assert r.status_code == 409
        assert r.headers.get("X-Existing-Document-ID") is not None

    def test_batch_upload(self):
        r = client.post(
            "/documents/batch",
            files=[
                ("files", ("a.pdf", b"content-a", "application/pdf")),
                ("files", ("b.pdf", b"content-b", "application/pdf")),
                ("files", ("bad.exe", b"content-c", "application/octet-stream")),
            ],
            data={"document_type": "other"},
            headers=_headers(),
        )
        assert r.status_code == 201
        results = r.json()["results"]
        assert [x["status"] for x in results] == ["uploaded", "uploaded", "failed"]


class TestRetrieval:
    def setup_method(self):
        _clear()
        self.doc_id = _upload().json()["id"]
        self.img_id = _upload(title="Scan", fname="scan.png", content=PNG_BYTES, doc_type="receipt").json()["id"]

    def test_get_document(self):
        r = client.get(f"/documents/{self.doc_id}", headers=_headers())
        assert r.status_code == 200
        assert r.json()["id"] == self.doc_id

    def test_download_document(self):
        r = client.get(f"/documents/{self.doc_id}/download", headers=_headers())
        assert r.status_code == 200
        assert r.content == PDF_BYTES

    def test_preview_image(self):
        r = client.get(f"/documents/{self.img_id}/preview", headers=_headers())
        assert r.status_code == 200
        assert r.content == PNG_BYTES

    def test_preview_non_image_400(self):
        r = client.get(f"/documents/{self.doc_id}/preview", headers=_headers())
        assert r.status_code == 400

    def test_missing_document_404(self):
        assert client.get("/documents/nope", headers=_headers()).status_code == 404

    def test_book_isolation(self):
        assert client.get(f"/documents/{self.doc_id}", headers=_headers(book=BOOK_B)).status_code == 404
        listed_a = client.get("/documents", headers=_headers(book=BOOK_A)).json()
        assert listed_a["total"] == 0

    def test_user_isolation(self):
        assert client.get(f"/documents/{self.doc_id}", headers=_headers(user=OTHER_USER)).status_code == 404
        assert client.get("/documents", headers=_headers(user=OTHER_USER)).json()["total"] == 0


class TestUpdateDelete:
    def setup_method(self):
        _clear()
        self.doc_id = _upload().json()["id"]

    def test_update_document(self):
        r = client.put(
            f"/documents/{self.doc_id}",
            json={"title": "Renamed", "tags": ["vat", "q4"], "description": "updated"},
            headers=_headers(),
        )
        assert r.status_code == 200
        data = r.json()
        assert data["title"] == "Renamed"
        assert data["tags"] == ["vat", "q4"]

        # persisted
        g = client.get(f"/documents/{self.doc_id}", headers=_headers()).json()
        assert g["title"] == "Renamed"
        assert g["tags"] == ["vat", "q4"]

    def test_update_missing_404(self):
        assert client.put("/documents/nope", json={"title": "x"}, headers=_headers()).status_code == 404

    def test_soft_delete_hides_everywhere(self):
        assert client.delete(f"/documents/{self.doc_id}", headers=_headers()).status_code == 204

        assert client.get(f"/documents/{self.doc_id}", headers=_headers()).status_code == 200  # record kept
        assert client.get("/documents", headers=_headers()).json()["total"] == 0  # but filtered from lists
        search = client.post("/documents/search", json={"query": "invoice"}, headers=_headers()).json()
        assert search["total"] == 0

        # download is 410 gone
        assert client.get(f"/documents/{self.doc_id}/download", headers=_headers()).status_code == 410

    def test_delete_missing_404(self):
        assert client.delete("/documents/nope", headers=_headers()).status_code == 404


class TestSearchAndList:
    def setup_method(self):
        _clear()
        self.a = _upload(title="Invoice Q3", tags="finance, q3").json()
        self.b = _upload(
            title="Rent contract", fname="contract.pdf", content=b"contract bytes", doc_type="contract", tags="legal"
        ).json()
        self.c = _upload(
            title="Receipt scan", fname="scan.png", content=PNG_BYTES, doc_type="receipt", tags="finance"
        ).json()

    def test_list_filters(self):
        all_docs = client.get("/documents", headers=_headers()).json()
        assert all_docs["total"] == 3

        by_type = client.get("/documents", params={"document_type": "receipt"}, headers=_headers()).json()
        assert by_type["total"] == 1

        by_tag = client.get("/documents", params={"tag": "finance"}, headers=_headers()).json()
        assert by_tag["total"] == 2

    def test_list_pagination(self):
        page = client.get("/documents", params={"limit": 2, "offset": 0}, headers=_headers()).json()
        assert len(page["documents"]) == 2

    def test_search_full_text(self):
        r = client.post("/documents/search", json={"query": "rent"}, headers=_headers()).json()
        assert r["total"] == 1
        assert r["documents"][0]["id"] == self.b["id"]

    def test_search_by_tags_and_type(self):
        r = client.post(
            "/documents/search", json={"tags": ["finance"], "document_type": "receipt"}, headers=_headers()
        ).json()
        assert r["total"] == 1
        assert r["documents"][0]["id"] == self.c["id"]

    def test_search_book_isolated(self):
        r = client.post("/documents/search", json={"query": "invoice"}, headers=_headers(book=BOOK_B)).json()
        assert r["total"] == 0


class TestOCRAndStats:
    def setup_method(self):
        _clear()
        self.doc_id = _upload().json()["id"]

    def test_ocr_background_completes(self):
        r = client.post(f"/documents/{self.doc_id}/ocr", headers=_headers())
        assert r.status_code == 200
        assert r.json()["status"] == "processing"

        # TestClient runs background tasks synchronously before returning
        doc = client.get(f"/documents/{self.doc_id}", headers=_headers()).json()
        assert doc["status"] == "ocr_completed"
        assert doc["ocr_confidence"] == 0.9
        assert "PDF" in doc["ocr_text"]

    def test_ocr_text_searchable(self):
        client.post(f"/documents/{self.doc_id}/ocr", headers=_headers())
        r = client.post("/documents/search", json={"query": "placeholder"}, headers=_headers()).json()
        assert r["total"] == 1

    def test_statistics(self):
        client.post(f"/documents/{self.doc_id}/ocr", headers=_headers())
        _upload(title="Second", fname="r.png", content=PNG_BYTES, doc_type="receipt")
        r = client.get("/statistics", headers=_headers())
        assert r.status_code == 200
        data = r.json()
        assert data["total_documents"] == 2
        assert data["by_type"]["invoice"] == 1
        assert data["by_type"]["receipt"] == 1
        assert "ocr_completed" in data["by_status"]

    def test_statistics_book_isolated(self):
        _upload(title="B book", book=BOOK_B)
        r = client.get("/statistics", headers=_headers(book=BOOK_A)).json()
        assert r["total_documents"] == 0  # setup doc is personal, not BOOK_A
        assert client.get("/statistics", headers=_headers(book=BOOK_B)).json()["total_documents"] == 1


class TestLinking:
    def setup_method(self):
        _clear()
        self.a = _upload().json()["id"]
        self.b = _upload(title="Second", fname="other.pdf", content=b"other bytes").json()["id"]

    def test_link_document(self):
        r = client.post(
            f"/documents/{self.a}/link",
            params={"entity_type": "journal_entry", "entity_id": "JE-001"},
            headers=_headers(),
        )
        assert r.status_code == 200

        doc = client.get(f"/documents/{self.a}", headers=_headers()).json()
        assert doc["linked_entity_type"] == "journal_entry"
        assert doc["linked_entity_id"] == "JE-001"

        # searchable by linked entity
        s = client.post("/documents/search", json={"linked_entity_id": "JE-001"}, headers=_headers()).json()
        assert s["total"] == 1

    def test_batch_link(self):
        r = client.post(
            "/documents/batch/link",
            json={"document_ids": [self.a, self.b, "nope"], "entity_type": "invoice", "entity_id": "INV-9"},
            headers=_headers(),
        )
        assert r.status_code == 200, r.text
        statuses = {x["document_id"]: x["status"] for x in r.json()["results"]}
        assert statuses[self.a] == "linked"
        assert statuses[self.b] == "linked"
        assert statuses["nope"] == "not_found"

        # persisted
        doc = client.get(f"/documents/{self.a}", headers=_headers()).json()
        assert doc["linked_entity_id"] == "INV-9"

    def test_batch_link_book_isolated(self):
        r = client.post(
            "/documents/batch/link",
            json={"document_ids": [self.a], "entity_type": "invoice", "entity_id": "INV-9"},
            headers=_headers(book=BOOK_B),
        )
        assert r.status_code == 200
        assert r.json()["results"][0]["status"] == "not_found"

    def test_link_missing_404(self):
        r = client.post(
            "/documents/nope/link",
            params={"entity_type": "invoice", "entity_id": "INV-1"},
            headers=_headers(),
        )
        assert r.status_code == 404
