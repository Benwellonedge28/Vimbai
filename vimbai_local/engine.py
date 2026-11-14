"""The Rust engine, running beside the local runtime for the services that live in it.

A service retired to the engine has no Python directory left, so the on-device profile cannot import it. It can still be
served: the engine is one small binary, it keeps its records in a file rather than a database, and this module starts it
and forwards the service's address to it the way the Go gateway forwards to the engine.

The engine is optional. A machine without the binary gets the Python services that are still on disk, which is why the
profile falls back rather than refusing to boot.
"""

import json
import os
import socket
import subprocess
import time

import httpx
from fastapi import FastAPI, Request
from fastapi.responses import Response

REPO_ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
SERVICES_JSON = os.path.join(REPO_ROOT, "gateway", "go", "config", "services.json")
DEFAULT_BINARY = os.path.join(REPO_ROOT, "rust", "target", "release", "vimbai-api")
# The engine answers a calculator and a store; both are under no prefix of their own, so it is asked for /health
# before anything else is forwarded to it.
STARTUP_SECONDS = 20


def binary_path():
    """Where the engine binary is, or None when this machine has not built one."""
    named = os.environ.get("VIMBAI_ENGINE_BIN")
    for candidate in (named, DEFAULT_BINARY):
        if candidate and os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    return None


def _free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def _entry(service):
    with open(SERVICES_JSON, encoding="utf-8") as fh:
        for row in json.load(fh)["services"]:
            if row["name"] == service:
                return row
    return None


class Engine:
    """The engine process, its port, and the store file it keeps."""

    def __init__(self, data_dir):
        self.data_dir = data_dir
        self.binary = binary_path()
        self.port = None
        self.process = None
        self.store_file = os.path.join(data_dir, "engine", "store.json")

    @property
    def running(self):
        return self.process is not None and self.process.poll() is None

    def start(self):
        """Boot the engine over the data dir's store file. Returns False when there is nothing to boot."""
        if self.running:
            return True
        if self.binary is None:
            return False
        os.makedirs(os.path.dirname(self.store_file), exist_ok=True)
        self.port = _free_port()
        environment = dict(os.environ)
        environment["VIMBAI_API_ADDR"] = f"127.0.0.1:{self.port}"
        environment["VIMBAI_STORE_FILE"] = self.store_file
        self.process = subprocess.Popen(
            [self.binary],
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        deadline = time.monotonic() + STARTUP_SECONDS
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                self.process = None
                return False
            try:
                answer = httpx.get(f"http://127.0.0.1:{self.port}/health", timeout=0.5)
                if answer.status_code == 200:
                    return True
            except httpx.HTTPError:
                time.sleep(0.1)
        self.stop()
        return False

    def stop(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
        self.process = None

    def forward(self, method, path, headers, body):
        """Ask the engine for one address, exactly as the Go gateway would."""
        answer = httpx.request(
            method,
            f"http://127.0.0.1:{self.port}{path}",
            headers=headers,
            content=body,
            timeout=30,
        )
        return answer


def mount(app, engine, path_prefix, service):
    """Serve one retired service's address from the engine, keeping the address the client already uses."""
    entry = _entry(service) or {}
    strips = bool(entry.get("strip_prefix", False))
    held = FastAPI()

    @held.api_route("/{rest:path}", methods=["GET", "POST", "PUT", "PATCH", "DELETE"])
    async def forward(rest: str, request: Request):
        from urllib.parse import urlencode

        target = "/" + rest if strips else "/" + path_prefix.strip("/") + "/" + rest
        if request.url.query:
            target = f"{target}?{urlencode(request.query_params)}"
        headers = {
            name: value
            for name, value in request.headers.items()
            if name.lower() in ("authorization", "content-type", "x-user-id", "x-book-id", "accept")
        }
        answer = engine.forward(request.method, target, headers, await request.body())
        return Response(
            content=answer.content,
            status_code=answer.status_code,
            media_type=answer.headers.get("content-type", "application/json"),
        )

    app.mount("/" + path_prefix.strip("/"), held)
