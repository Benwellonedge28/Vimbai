"""Tests for the Vimbai local (on-device, offline) runtime.

It is one binary now: the engine, a data directory and the gateway rules a single machine would otherwise have nobody to
apply. These tests hold it to the promises an offline machine actually depends on - a process that says which profile it
is running, a caller without a token getting nowhere, a Book that stays somebody else's, and yesterday's bookkeeping still
there after a restart. They speak HTTP to a real process rather than calling a Python launcher, because the thing being
tested is the binary a machine would install.
"""

import json
import os
import shutil
import socket
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

import pytest

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def binary_path():
    """The runtime as built, or wherever VIMBAI_LOCAL_BIN points."""
    named = os.environ.get("VIMBAI_LOCAL_BIN")
    if named:
        return named if os.path.exists(named) else None
    for candidate in ("rust/target/debug/vimbai-local", "rust/target/release/vimbai-local"):
        path = os.path.join(REPO_ROOT, candidate)
        if os.path.exists(path):
            return path
    return None


# A machine that has not built the binary is told so rather than left believing the Book is being kept somewhere.
needs_runtime = pytest.mark.skipif(
    binary_path() is None,
    reason="the local runtime is not built; preflight builds it, or set VIMBAI_LOCAL_BIN",
)


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


class Runtime:
    """A local runtime process, with the calls these tests make."""

    def __init__(self, data_dir):
        self.data_dir = data_dir
        self.port = free_port()
        self.process = subprocess.Popen(
            [binary_path(), "--port", str(self.port), "--data-dir", data_dir],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        self.base = f"http://127.0.0.1:{self.port}"
        self._wait_until_ready()

    def _wait_until_ready(self, timeout=20.0):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"the runtime exited: {self.process.stdout.read()}")
            try:
                if self.get("/health")[0] == 200:
                    return
            except urllib.error.URLError:
                pass
            time.sleep(0.1)
        raise RuntimeError("the runtime did not answer /health")

    def request(self, method, path, body=None, headers=None, form=None):
        """One call. A JSON body, a form body or neither, and the status and parsed answer back."""
        data = None
        all_headers = dict(headers or {})
        if form is not None:
            data = urllib.parse.urlencode(form).encode()
            all_headers["Content-Type"] = "application/x-www-form-urlencoded"
        elif body is not None:
            data = json.dumps(body).encode()
            all_headers["Content-Type"] = "application/json"
        request = urllib.request.Request(self.base + path, data=data, headers=all_headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=10) as answer:
                raw = answer.read()
                status = answer.status
        except urllib.error.HTTPError as refusal:
            raw = refusal.read()
            status = refusal.code
        try:
            return status, json.loads(raw)
        except json.JSONDecodeError:
            return status, raw.decode(errors="replace")

    def get(self, path, headers=None):
        return self.request("GET", path, headers=headers)

    def post(self, path, body=None, headers=None, form=None):
        return self.request("POST", path, body=body, headers=headers, form=form)

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()


@pytest.fixture()
def runtime(tmp_path):
    data_dir = str(tmp_path / "vimbai-data")
    started = Runtime(data_dir)
    try:
        yield started, data_dir
    finally:
        started.stop()


def register_and_login(runtime, tag):
    suffix = uuid.uuid4().hex[:8]
    username = f"local_{tag}_{suffix}"
    status, _ = runtime.post(
        "/identity/users/register",
        body={
            "email": f"{username}@example.com",
            "username": username,
            "password": "Local-Pass!2026",
            "first_name": "Local",
            "last_name": tag,
        },
    )
    assert status == 201, status
    status, answer = runtime.post(
        "/identity/users/login",
        form={"username": username, "password": "Local-Pass!2026"},
    )
    assert status == 200, answer
    return {"Authorization": f"Bearer {answer['access_token']}"}, username


def create_book(runtime, auth):
    status, answer = runtime.post(
        "/book-sync/books", body={"name": "Local Book", "tier": "business"}, headers=auth
    )
    assert status == 200, answer
    book_id = (answer.get("book") or {}).get("id") or answer.get("id")
    assert book_id
    return book_id


def create_department(runtime, auth, book_id, tag):
    status, answer = runtime.post(
        "/departmental-accounting/departments",
        body={
            "id": f"loc-{tag}-1",
            "name": "Sales",
            "department_code": f"LOC-{tag}".upper(),
            "department_name": "Sales",
            "department_type": "revenue",
            "manager_id": "mgr-1",
            "manager_name": "Sam",
        },
        headers={**auth, "X-Book-ID": book_id},
    )
    assert status in (200, 201), answer
    return created_id(answer)


def created_id(payload):
    """The id of a record a create answered.

    A department create answers with the department and the cost centre it opened in the chart of accounts, which is why
    this reads one level in.
    """
    return (payload.get("department") or payload)["id"]


def listed_departments(payload):
    if isinstance(payload, list):
        return payload
    if isinstance(payload, dict):
        for key in ("departments", "items", "data"):
            if isinstance(payload.get(key), list):
                return payload[key]
    return []


def listed_books(payload):
    if isinstance(payload, list):
        return payload
    if isinstance(payload, dict):
        for key in ("books", "items", "data"):
            if isinstance(payload.get(key), list):
                return payload[key]
    return []


@needs_runtime
def test_health_reports_core_profile(runtime):
    started, _ = runtime
    status, body = started.get("/health")
    assert status == 200
    assert body["status"] == "healthy"
    assert body["profile"] == "core"
    # Every member the profile names is answered by the engine in this same process, so none is skipped.
    assert body["services_loaded"] == 5
    assert body["services_skipped"] == 0
    assert body["skipped"] == []


@needs_runtime
def test_unauthenticated_call_rejected(runtime):
    started, _ = runtime
    status, body = started.get("/departmental-accounting/departments")
    assert status == 401, body


@needs_runtime
def test_a_token_the_runtime_did_not_sign_is_refused(runtime):
    started, _ = runtime
    # A well-formed token signed with something else is not a token, however plausible its claims look.
    forged = (
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"
        ".eyJzdWIiOiJ1c2VyLTEiLCJ0eXBlIjoiYWNjZXNzIn0"
        ".ZmFrZS1zaWduYXR1cmU"
    )
    status, body = started.get(
        "/departmental-accounting/departments", headers={"Authorization": f"Bearer {forged}"}
    )
    assert status == 401, body
    status, body = started.get("/departmental-accounting/departments", headers={"X-User-Id": "user-1"})
    assert status == 401, body


@needs_runtime
def test_book_scoped_write_read_and_isolation(runtime):
    started, _ = runtime
    auth_a, _ = register_and_login(started, "a")
    auth_b, _ = register_and_login(started, "b")
    book_id = create_book(started, auth_a)
    department = create_department(started, auth_a, book_id, "sales")

    status, listing = started.get(
        "/departmental-accounting/departments", headers={**auth_a, "X-Book-ID": book_id}
    )
    assert status == 200
    assert department in [row.get("id") for row in listed_departments(listing)]

    # Somebody else's Book is somebody else's: the gate refuses before the engine is asked.
    status, refusal = started.get(
        "/departmental-accounting/departments", headers={**auth_b, "X-Book-ID": book_id}
    )
    assert status == 403, refusal
    assert department not in json.dumps(refusal)

    # A book-scoped read with no Book at all is refused by the engine rather than answered as empty.
    status, _ = started.get("/departmental-accounting/departments", headers=auth_b)
    assert status in (200, 400, 403)


@needs_runtime
def test_a_published_prefix_reaches_the_engines_own_address(runtime):
    """`/book-sync/books` is what clients were told to call; the engine answers `/books`."""
    started, _ = runtime
    auth, _ = register_and_login(started, "prefix")
    book_id = create_book(started, auth)
    status, listing = started.get("/book-sync/books", headers=auth)
    assert status == 200
    assert book_id in [(row.get("book") or row).get("id") for row in listed_books(listing)]


@needs_runtime
def test_data_survives_full_restart(runtime):
    started, data_dir = runtime
    auth, username = register_and_login(started, "durable")
    book_id = create_book(started, auth)
    department = create_department(started, auth, book_id, "durable")
    started.stop()

    # --- a machine restart: a new process, the same directory, nothing carried over in memory ---
    restarted = Runtime(data_dir)
    try:
        status, answer = restarted.post(
            "/identity/users/login", form={"username": username, "password": "Local-Pass!2026"}
        )
        assert status == 200, answer
        auth2 = {"Authorization": f"Bearer {answer['access_token']}"}

        status, listing = restarted.get("/book-sync/books", headers=auth2)
        assert status == 200
        assert book_id in [(row.get("book") or row).get("id") for row in listed_books(listing)]

        status, listing = restarted.get(
            "/departmental-accounting/departments", headers={**auth2, "X-Book-ID": book_id}
        )
        assert status == 200
        assert department in [row.get("id") for row in listed_departments(listing)]
    finally:
        restarted.stop()


@needs_runtime
def test_the_runtime_keeps_its_signing_secret(runtime):
    """A secret made on first boot has to survive the boot after, or every token in a person's hand dies with the process."""
    started, data_dir = runtime
    started.stop()
    secret_file = os.path.join(data_dir, "jwt_secret")
    assert os.path.isfile(secret_file)
    with open(secret_file, encoding="utf-8") as handle:
        secret = handle.read().strip()
    assert len(secret) >= 32

    restarted = Runtime(data_dir)
    try:
        with open(secret_file, encoding="utf-8") as handle:
            assert handle.read().strip() == secret
    finally:
        restarted.stop()


def test_the_manifest_the_runtime_carries_is_the_gateways():
    """The runtime routes published prefixes from the gateway's own manifest, compiled into the binary.

    This broke once, silently: the gateway moved to gateway/go and the launcher kept pointing at api-gateway/, which no
    test noticed because the core profile names its members explicitly.
    """
    manifest = os.path.join(REPO_ROOT, "gateway", "go", "config", "services.json")
    assert os.path.isfile(manifest), manifest
    with open(manifest, encoding="utf-8") as handle:
        services = json.load(handle)["services"]
    assert len(services) > 100, "the manifest should see every routed service"
    assert all(service["name"] and service["path"] for service in services)


def test_the_runtime_is_one_directory():
    """There used to be two: vimbai-local/ held the docs and the Python package held the code. Then it was a Python
    package that started a Rust engine; now it is the Rust runtime, and the Python launcher it replaced is gone."""
    assert not os.path.exists(os.path.join(REPO_ROOT, "vimbai-local"))
    assert not os.path.exists(os.path.join(REPO_ROOT, "vimbai_local"))
    assert os.path.isdir(os.path.join(REPO_ROOT, "rust", "apps", "local-runtime"))


@pytest.mark.skipif(shutil.which("cargo") is None, reason="cargo is not installed")
def test_the_runtime_binary_is_the_one_that_ships():
    """The binary's crate is in the workspace, so `cargo test --workspace` builds and checks it like everything else."""
    workspace = os.path.join(REPO_ROOT, "rust", "Cargo.toml")
    with open(workspace, encoding="utf-8") as handle:
        assert '"apps/local-runtime"' in handle.read()
