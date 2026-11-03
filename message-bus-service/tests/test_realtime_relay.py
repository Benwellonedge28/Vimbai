"""The bus pushes every event to the WebSocket relay.

Without this hop, a published event stayed on the bus: only webhook
subscribers and alert rules reacted, and a signed-in client learned about a
write on its next poll.
"""

import os
import sys
from pathlib import Path

import pytest

SERVICE_DIR = Path(__file__).resolve().parents[1]
if str(SERVICE_DIR) not in sys.path:
    sys.path.insert(0, str(SERVICE_DIR))

os.environ.setdefault("NEO4J_URI", "bolt://localhost:7687")

import main  # noqa: E402,F401 - bootstraps the message_bus_service package alias
from message_bus_service import dispatch  # noqa: E402
from message_bus_service.models import Event, EventType  # noqa: E402


def _event(book_id="book-1"):
    return Event(
        type=EventType.BOOK_RESOURCE_CREATED,
        source_service="api-gateway",
        payload={"book_id": book_id, "resource_type": "journal_entry", "resource_id": "je-1"},
    )


class FakeResponse:
    status_code = 200


class Recorder:
    calls = []

    def __init__(self, *args, **kwargs):
        pass

    async def __aenter__(self):
        return self

    async def __aexit__(self, *exc):
        return False

    async def post(self, url, json=None, **kwargs):
        Recorder.calls.append((url, json))
        return FakeResponse()


@pytest.fixture(autouse=True)
def reset():
    Recorder.calls = []
    yield
    Recorder.calls = []


def test_event_is_forwarded_to_the_relay(monkeypatch):
    monkeypatch.setattr(dispatch.httpx, "AsyncClient", Recorder)
    import asyncio

    asyncio.get_event_loop_policy().new_event_loop().run_until_complete(dispatch._deliver_realtime(_event()))
    url, body = Recorder.calls[0]
    assert url.endswith("/events/ingest")
    assert body["payload"]["book_id"] == "book-1"
    # The bus envelope names the type "type"; the relay normaliser accepts either.
    assert body["type"] == "book.resource.created"


def test_relay_failure_never_raises(monkeypatch):
    class Boom(Recorder):
        async def post(self, url, json=None, **kwargs):
            raise RuntimeError("relay down")

    monkeypatch.setattr(dispatch.httpx, "AsyncClient", Boom)
    import asyncio

    asyncio.get_event_loop_policy().new_event_loop().run_until_complete(dispatch._deliver_realtime(_event()))


def test_relay_can_be_switched_off(monkeypatch):
    monkeypatch.setattr(dispatch, "REALTIME_RELAY_MODE", "off")
    monkeypatch.setattr(dispatch.httpx, "AsyncClient", Recorder)
    import asyncio

    asyncio.get_event_loop_policy().new_event_loop().run_until_complete(dispatch._deliver_realtime(_event()))
    assert Recorder.calls == []
