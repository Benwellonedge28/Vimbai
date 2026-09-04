"""Multimodal Pipeline Service - Book context (X-Book-ID) tests.

The Neo4j backend is mocked: these tests verify that every query the CRUD
layer runs carries the Book context parameter, and that Book filtering is
applied to reads, writes, updates and deletes.
"""

import pytest
from multimodal_pipeline_service import crud
from multimodal_pipeline_service.dependencies import book_id_var
from multimodal_pipeline_service.models import MultimodalProcessingTaskCreate, UserCorrection


class FakeResult:
    def __init__(self, records=None):
        self.records = records or []

    async def single(self):
        return self.records[0] if self.records else None

    def __aiter__(self):
        return self._gen()

    async def _gen(self):
        for rec in self.records:
            yield rec

    def consume(self):
        class Counters:
            nodes_deleted = 0

        class Summary:
            counters = Counters()

        return Summary()


class FakeSession:
    """Captures (query, params) pairs instead of touching Neo4j."""

    def __init__(self, records=None):
        self.calls = []
        self.records = records or []

    async def run(self, query, params=None, **kw):
        merged = dict(params or {})
        merged.update(kw)
        self.calls.append((query, merged))
        return FakeResult(self.records)


def _task_record(task_create, **extra):
    """A Neo4j-shaped record complete enough for model reconstruction."""
    base = task_create.model_dump()
    base.update(
        {
            "id": "t1",
            "created_at": "2026-01-01T00:00:00+00:00",
            "updated_at": "2026-01-01T00:00:00+00:00",
        }
    )
    base.update(extra)
    return {"mpt": base}


@pytest.fixture
def task_create():
    return MultimodalProcessingTaskCreate(
        input_type="text",
        input_data="Sample invoice text",
        user_id="test-user-id",
    )


async def test_queries_carry_book_id_when_set(task_create):
    session = FakeSession()
    token = book_id_var.set("book-123")
    try:
        session.records = [_task_record(task_create, book_id="book-123")]
        await crud.create_multimodal_processing_task(session, task_create)
        query, params = session.calls[0]
        # create stamps book_id inside the node properties
        assert params["props"]["book_id"] == "book-123"

        await crud.get_all_multimodal_processing_tasks(session, "u")
        query, params = session.calls[1]
        assert params["book_id"] == "book-123"
        assert "mpt.book_id = $book_id" in query

        await crud.get_multimodal_processing_task(session, "t1")
        query, params = session.calls[2]
        assert "$book_id IS NULL OR mpt.book_id = $book_id" in query

        await crud.delete_multimodal_processing_task(session, "t1")
        query, params = session.calls[3]
        assert "$book_id IS NULL OR mpt.book_id = $book_id" in query
    finally:
        book_id_var.reset(token)


async def test_queries_book_id_none_when_unscoped(task_create):
    session = FakeSession()
    session.records = [_task_record(task_create)]
    await crud.create_multimodal_processing_task(session, task_create)
    _, params = session.calls[0]
    assert params["props"]["book_id"] is None

    await crud.get_all_multimodal_processing_tasks(session, "u")
    query, params = session.calls[1]
    assert params["book_id"] is None
    assert "mpt.book_id = $book_id" in query


async def test_correction_create_guards_parent_task_book():
    session = FakeSession()
    correction_props = {
        "id": "c1",
        "task_id": "t1",
        "user_id": "u1",
        "field_name": "amount",
        "original_value": "100",
        "corrected_value": "150",
        "feedback_type": "value_correction",
        "submitted_at": "2026-01-01T00:00:00+00:00",
    }
    session.records = [{"uc": correction_props}]
    correction = UserCorrection(
        task_id="t1",
        user_id="u1",
        field_name="amount",
        original_value="100",
        corrected_value="150",
        feedback_type="value_correction",
    )
    token = book_id_var.set("book-abc")
    try:
        await crud.create_user_correction(session, correction)
        query, params = session.calls[0]
        assert params["book_id"] == "book-abc"
        assert "WHERE $book_id IS NULL OR mpt.book_id = $book_id" in query
    finally:
        book_id_var.reset(token)
