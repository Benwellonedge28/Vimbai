"""Vimbai local runtime: the whole platform in one process, no server needed.

Boots the registered Vimbai services as a single FastAPI application with
production-parity gateway behaviour (JWT auth, X-User-ID injection,
Book-context membership gating), file-backed persistence instead of Neo4j,
and sqlite for book-sync. Runs on any machine with Python 3.11+ - Windows,
macOS or Linux - fully offline.

    python3 -m vimbai_local                      # core bookkeeping profile
    python3 -m vimbai_local --profile full       # every registered service
    python3 -m vimbai_local --port 9000 --data-dir ~/vimbai-data
"""

import importlib.util
import json
import os
import sys

import uvicorn
from fastapi import FastAPI

_HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.normpath(os.path.join(_HERE, ".."))
SERVICES_JSON = os.path.join(REPO_ROOT, "api-gateway", "config", "services.json")

# identity-service is registered as the gateway's open /identity route
# (api-gateway/config/config.go), not in services.json.
IDENTITY = ("identity", "identity-service")

# Core bookkeeping loop: chart of accounts + double-entry journal
# (accounting), departmental accounting, and audit/compliance trail.
CORE_MEMBERS = (
    ("accounting", "accounting-service"),
    ("departmental-accounting", "departmental-accounting-service"),
    ("audit-compliance", "audit-compliance-service"),
    IDENTITY,
    ("book-sync", "book-sync-service"),
)


def _load_services_json():
    with open(SERVICES_JSON, encoding="utf-8") as fh:
        return json.load(fh)["services"]


def _prepare_environment(data_dir, port):
    """All filesystem-backed services point into the data dir; services that
    call sibling services reach them through this same process."""
    os.makedirs(data_dir, exist_ok=True)
    os.environ["BOOK_SYNC_DB"] = os.path.join(data_dir, "book_sync.db")
    os.environ["PERSONAL_FINANCE_DB"] = os.path.join(data_dir, "personal_finance.db")
    os.environ["NPO_SCALE_DB"] = os.path.join(data_dir, "npo_scale.db")
    os.environ["ACCOUNTING_SERVICE_URL"] = f"http://127.0.0.1:{port}/accounting"
    # JWT secret must exist before any service import (some read it at
    # import time); identity-service re-reads the env at call time.
    from vimbai_local.gateway import load_or_create_secret

    os.environ["JWT_SECRET"] = load_or_create_secret(data_dir)


def _module_name(service_dir):
    return "vimbai_local_" + service_dir.replace("-", "_")


def _import_service(service_dir):
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    mod_name = _module_name(service_dir)
    if mod_name in sys.modules:
        return sys.modules[mod_name]
    # Services with an __init__.py import siblings via the underscore package
    # name (e.g. `from invoicing_service import crud`); the on-disk dir is
    # hyphenated, so register the package alias with the right __path__.
    pkg = service_dir.replace("-", "_")
    if os.path.isfile(os.path.join(REPO_ROOT, service_dir, "__init__.py")) and pkg not in sys.modules:
        pkg_spec = importlib.util.spec_from_file_location(pkg, os.path.join(REPO_ROOT, service_dir, "__init__.py"))
        pkg_mod = importlib.util.module_from_spec(pkg_spec)
        pkg_mod.__path__ = [os.path.join(REPO_ROOT, service_dir)]
        sys.modules[pkg] = pkg_mod
        pkg_spec.loader.exec_module(pkg_mod)
    spec = importlib.util.spec_from_file_location(mod_name, main_path)
    mod = importlib.util.module_from_spec(spec)
    # Register before exec: some libs inspect sys.modules during class
    # definition (same lesson as the bracket containers).
    sys.modules[mod_name] = mod
    spec.loader.exec_module(mod)
    return mod


def _service_pkg(service_dir):
    return service_dir.replace("-", "_")


def build_app(profile="core", data_dir=None, port=8000, host="127.0.0.1"):
    """Assemble the composite local application. Returns (app, loaded, skipped)."""
    from vimbai_local import store as store_mod
    from vimbai_local.gateway import LocalGatewayMiddleware, load_or_create_secret

    data_dir = os.path.abspath(data_dir or "./vimbai-data")
    _prepare_environment(data_dir, port)

    if profile == "full":
        members = [(s["path"].lstrip("/"), s["name"]) for s in _load_services_json()]
        members.append(IDENTITY)
    elif profile == "core":
        members = list(CORE_MEMBERS)
    else:
        raise ValueError(f"unknown profile: {profile!r} (use 'core' or 'full')")

    app = FastAPI(
        title="Vimbai Local",
        version="1.0.0",
        description="Vimbai on-device runtime - single process, offline, file-backed.",
    )

    loaded, skipped, persisted = [], [], []
    book_sync_db_path = None
    for path_prefix, service_dir in members:
        try:
            mod = _import_service(service_dir)
            pkg = _service_pkg(service_dir)
            # Sqlite-backed services read their DB path at import time; a
            # later boot with the same process must re-target the data dir.
            if hasattr(mod, "DB_PATH"):
                mod.DB_PATH = os.environ["BOOK_SYNC_DB"]
                if hasattr(mod, "init_db"):
                    mod.init_db()
            if hasattr(mod, "DB_PATH") and getattr(mod, "DB_PATH", None):
                if service_dir == "book-sync-service":
                    book_sync_db_path = mod.DB_PATH
            if store_mod.attach_service_store(os.path.join(REPO_ROOT, service_dir), pkg, data_dir):
                persisted.append(service_dir)
            app.mount("/" + path_prefix, mod.app)
            loaded.append(f"/{path_prefix}")
        except Exception as exc:  # noqa: BLE001 - one bad service must not sink boot
            skipped.append((f"/{path_prefix}", repr(exc)))

    book_sync_db = book_sync_db_path or os.environ["BOOK_SYNC_DB"]
    app.add_middleware(
        LocalGatewayMiddleware,
        jwt_secret=os.environ["JWT_SECRET"],
        book_sync_db=book_sync_db,
    )

    @app.get("/health")
    def local_health():
        return {
            "service": "vimbai-local",
            "profile": profile,
            "status": "healthy",
            "data_dir": data_dir,
            "services_loaded": len(loaded),
            "services_skipped": len(skipped),
            "skipped": [s[0] for s in skipped],
        }

    @app.get("/")
    def local_root():
        return {"service": "vimbai-local", "docs": "/docs", "health": "/health"}

    return app, loaded, skipped, persisted


def main(argv=None):
    import argparse

    parser = argparse.ArgumentParser(
        prog="vimbai_local",
        description="Run the entire Vimbai platform locally, offline, on one process.",
    )
    parser.add_argument(
        "--profile",
        choices=("core", "full"),
        default="core",
        help="core = essential bookkeeping loop (default); full = every registered service",
    )
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument(
        "--data-dir", default="./vimbai-data", help="where local data (books, stores, JWT secret) lives"
    )
    args = parser.parse_args(argv)

    app, loaded, skipped, persisted = build_app(
        profile=args.profile, data_dir=args.data_dir, port=args.port, host=args.host
    )
    print(f"Vimbai local runtime - profile={args.profile}")
    print(f"  loaded {len(loaded)} services ({len(persisted)} with local persistence)")
    if skipped:
        print(f"  skipped {len(skipped)} services:")
        for name, err in skipped:
            print(f"    {name}: {err}")
    print(f"  data dir: {os.path.abspath(args.data_dir)}")
    print(f"  listening on http://{args.host}:{args.port} (docs at /docs)")
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
