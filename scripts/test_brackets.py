#!/usr/bin/env python3
"""Smoke test: every bracket app imports all members, /health works,
and one mounted sub-service route responds per bracket."""

import glob
import importlib.util
import os
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO, "brackets"))

from fastapi.testclient import TestClient  # noqa: E402

failures = []
for bdir in sorted(glob.glob(os.path.join(REPO, "brackets", "*"))):
    name = os.path.basename(bdir)
    spec = importlib.util.spec_from_file_location(name + "_app", os.path.join(bdir, "main.py"))
    try:
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
    except Exception as e:
        failures.append((name, f"import failed: {type(e).__name__}: {e}"))
        continue

    client = TestClient(mod.app)
    h = client.get("/health")
    ok_health = h.status_code == 200 and h.json().get("status") == "healthy"
    mounted = h.json().get("mounted_services", 0) if ok_health else 0
    expected = len(mod.MEMBERS)
    print(f"{name}: health={'OK' if ok_health else 'FAIL'} mounted={mounted}/{expected}")

    if not ok_health or mounted != expected:
        failures.append((name, f"health/mount mismatch {mounted}/{expected}"))
        continue

    # probe one member route per bracket: /health, / or first GET route
    prefix = mod.MEMBERS[0][0]
    sub = mod.app.routes
    paths = ["/health", "/"]
    for route in sub:
        m = getattr(route, "methods", None)
        if m and "GET" in m and hasattr(route, "path") and "{" not in route.path:
            paths.append(route.path)
    if not any((r := client.get(f"/{prefix}{p}")).status_code == 200 for p in paths):
        failures.append((name, f"member /{prefix} no responding GET route: {paths[:3]}"))

print()
if failures:
    print("FAILURES:")
    for n, msg in failures:
        print(f"  {n}: {msg}")
    sys.exit(1)
print("ALL BRACKETS PASS")
