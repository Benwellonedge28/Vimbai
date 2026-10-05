"""
Vimbai Book Inbox Tests
Incomplete records wired to each Book: inbox listing, summary counts,
organize (accept AI result into the Book), video/text processing branches.
"""

import asyncio
import os
from datetime import datetime, timedelta, timezone
from unittest.mock import AsyncMock, patch

import pytest
from fastapi.testclient import TestClient
from jose import jwt as pyjwt

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")

from main import app  # noqa: E402
from multimodal_pipeline_service import crud, models  # noqa: E402

client = TestClient(app)


def _token(user_id="test-user-id", permissions=None):
    permissions = permissions or [
        "multimodal.read.tasks",
        "multimodal.write.tasks",
        "multimodal.delete.tasks",
        "multimodal.read.corrections",
        "multimodal.write.corrections",
    ]
    return pyjwt.encode(
        {
            "user_id": user_id,
            "username": "testuser",
            "role": "admin",
            "permissions": permissions,
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )


@pytest.fixture
def auth_headers():
    return {"Authorization": f"Bearer {_token()}"}


@pytest.fixture
def book_headers(auth_headers):
    return {**auth_headers, "X-Book-ID": "book-1"}


def _task(status, input_type="image", user_id="test-user-id", task_id="t-1"):
    return models.MultimodalProcessingTaskInDB(
        id=task_id,
        user_id=user_id,
        input_type=input_type,
        status=status,
        created_at=datetime.now(timezone.utc),
        updated_at=datetime.now(timezone.utc),
    )


# ---------------------------------------------------------------------------
# Endpoint contracts
# ---------------------------------------------------------------------------


class TestInboxEndpoints:
    def test_inbox_requires_auth(self):
        assert client.get("/inbox").status_code in (401, 403)
        assert client.get("/inbox/summary").status_code in (401, 403)
        assert client.post("/inbox/t-1/organize").status_code in (401, 403)

    def test_inbox_rejects_invalid_view(self, book_headers):
        r = client.get("/inbox", params={"view": "bogus"}, headers=book_headers)
        assert r.status_code == 422
        assert "view" in r.json()["detail"]

    def test_inbox_lists_incomplete_by_default(self, book_headers):
        tasks = [_task("review_pending"), _task("received", task_id="t-2")]
        with patch.object(crud, "list_inbox_tasks", AsyncMock(return_value=tasks)) as m:
            r = client.get("/inbox", headers=book_headers)
        assert r.status_code == 200
        body = r.json()
        assert len(body) == 2
        assert body[0]["status"] == "review_pending"
        m.assert_awaited_once()
        assert m.await_args.args[2] == "incomplete"  # default view

    def test_inbox_summary_passthrough(self, book_headers):
        summary = {"review_pending": 2, "completed": 1, "total_incomplete": 2, "total_organized": 1}
        with patch.object(crud, "inbox_summary", AsyncMock(return_value=summary)):
            r = client.get("/inbox/summary", headers=book_headers)
        assert r.status_code == 200
        assert r.json() == summary

    def test_organize_accepts_ai_result_into_book(self, book_headers):
        done = _task("completed")
        with (
            patch.object(crud, "get_multimodal_processing_task", AsyncMock(return_value=_task("ai_extracted"))),
            patch.object(crud, "update_multimodal_processing_task", AsyncMock(return_value=done)) as upd,
        ):
            r = client.post("/inbox/t-1/organize", headers=book_headers)
        assert r.status_code == 200
        assert r.json()["status"] == "completed"
        update_arg = upd.await_args.args[2]
        assert update_arg.status == "completed"
        assert update_arg.processing_end_time is not None

    def test_organize_is_idempotent(self, book_headers):
        done = _task("completed")
        with (
            patch.object(crud, "get_multimodal_processing_task", AsyncMock(return_value=done)),
            patch.object(crud, "update_multimodal_processing_task", AsyncMock()) as upd,
        ):
            r = client.post("/inbox/t-1/organize", headers=book_headers)
        assert r.status_code == 200
        assert r.json()["status"] == "completed"
        upd.assert_not_awaited()  # already organized: no rewrite

    def test_organize_foreign_record_404(self, book_headers):
        foreign = _task("ai_extracted", user_id="someone-else")
        with patch.object(crud, "get_multimodal_processing_task", AsyncMock(return_value=foreign)):
            r = client.post("/inbox/t-1/organize", headers=book_headers)
        assert r.status_code == 404

    def test_organize_missing_record_404(self, book_headers):
        with patch.object(crud, "get_multimodal_processing_task", AsyncMock(return_value=None)):
            r = client.post("/inbox/t-1/organize", headers=book_headers)
        assert r.status_code == 404


# ---------------------------------------------------------------------------
# Inbox crud logic (view filtering + summary counts)
# ---------------------------------------------------------------------------


class TestInboxCrudLogic:
    def _seed(self):
        return [
            _task("review_pending", task_id="t-1"),
            _task("processing", task_id="t-2"),
            _task("received", task_id="t-3"),
            _task("ai_extracted", task_id="t-4"),
            _task("failed", task_id="t-5"),
            _task("completed", task_id="t-6"),
            _task("completed", task_id="t-7"),
        ]

    def test_list_inbox_views(self):
        seed = self._seed()
        with patch.object(crud, "get_all_multimodal_processing_tasks", AsyncMock(return_value=seed)):
            incomplete = asyncio.run(crud.list_inbox_tasks(None, "u1", "incomplete"))
            organized = asyncio.run(crud.list_inbox_tasks(None, "u1", "organized"))
            everything = asyncio.run(crud.list_inbox_tasks(None, "u1", "all"))
        assert {t.id for t in incomplete} == {"t-1", "t-2", "t-3", "t-4", "t-5"}
        assert {t.id for t in organized} == {"t-6", "t-7"}
        assert len(everything) == 7

    def test_list_inbox_limit(self):
        seed = self._seed()
        with patch.object(crud, "get_all_multimodal_processing_tasks", AsyncMock(return_value=seed)):
            limited = asyncio.run(crud.list_inbox_tasks(None, "u1", "all", limit=3))
        assert len(limited) == 3

    def test_inbox_summary_counts(self):
        seed = self._seed()
        with patch.object(crud, "get_all_multimodal_processing_tasks", AsyncMock(return_value=seed)):
            summary = asyncio.run(crud.inbox_summary(None, "u1"))
        assert summary["review_pending"] == 1
        assert summary["processing"] == 1
        assert summary["received"] == 1
        assert summary["failed"] == 1
        assert summary["completed"] == 2
        assert summary["total_incomplete"] == 5
        assert summary["total_organized"] == 2


# ---------------------------------------------------------------------------
# Background AI processing: video and text capture branches
# ---------------------------------------------------------------------------


class TestVideoAndTextProcessing:
    def _run_processor(self, task):
        from multimodal_pipeline_service.services.ai_processor import AIProcessor

        updates = []

        async def fake_get(session, task_id):
            return task

        async def fake_update(session, task_id, update):
            updates.append(update)
            return task

        async def fast_sleep(delay):
            return None

        with (
            patch("multimodal_pipeline_service.crud.get_multimodal_processing_task", fake_get),
            patch("multimodal_pipeline_service.crud.update_multimodal_processing_task", fake_update),
            patch("asyncio.sleep", fast_sleep),
        ):
            asyncio.run(AIProcessor(None).process_multimodal_task(task.id))
        return updates

    def test_video_capture_lands_in_review_with_suggested_entry(self):
        task = _task("received", input_type="video", task_id="v-1")
        task.input_url = "data:video/mp4;base64,AAAA"
        updates = self._run_processor(task)
        assert updates, "processor must persist at least one status update"
        final = updates[-1]
        assert final.status == "review_pending"  # 0.68 confidence < 0.9
        assert final.document_result is not None
        assert final.suggested_journal_entry["source"] == "camera_video"
        assert final.suggested_journal_entry["amount"] == "12.50"

    def test_text_capture_keeps_raw_text(self):
        task = _task("received", input_type="text", task_id="x-1")
        task.input_raw_text = "paid school fees 200 usd"
        updates = self._run_processor(task)
        final = updates[-1]
        assert final.status == "review_pending"
        assert final.document_result.raw_text == "paid school fees 200 usd"
        assert final.suggested_journal_entry is None
