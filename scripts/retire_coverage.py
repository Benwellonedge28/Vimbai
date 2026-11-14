"""Which Python services the Rust engine already answers, path by path.

The retire gate was written for the native seam: it asks which calculator a service's calc.py calls and whether the
extension carries it. That is no longer the question. The engine is an HTTP surface now, and a service may only be
deleted once every fixed path it publishes is answered by the Rust API server - under the same prefix the gateway
publishes, or flat, which are different addresses and have to be told apart because they decide whether a service's
gateway entry keeps its prefix or drops it.

An endpoint whose path carries a parameter is not counted: the parameter is a hole in the path, and a hole matches
anything, so counting it as coverage would let a service retire on the strength of a route that may answer something
else. Fixed paths only.

Run from the repository root:  python3 scripts/retire_coverage.py [domain ...]
Exit code 0 when at least one service is ready to retire, 1 when none is.
"""

import importlib.util
import json
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REGISTRY = os.path.join(REPO, "gateway", "go", "config", "services.json")
API_DIR = os.path.join(REPO, "rust", "apps", "api-server", "src")

ROUTE = re.compile(r'\.route\(\s*"([^"]+)"')
PREFIXED = re.compile(r'format!\("(?:\{prefix\}|([^"]*))([^"]*)"')
ENDPOINT = re.compile(r'@app\.(?:get|post|put|delete|patch)\(\s*[\'"]([^\'"]+)[\'"]')

# the mounts a prefix loop registers under, for the surfaces that build one table and hang it in two places
MOUNTS = ("/accounting", "/records/accounting")


def manifest():
    path = os.path.join(REPO, "scripts", "migration_manifest.py")
    spec = importlib.util.spec_from_file_location("migration_manifest", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def api_routes():
    """Every path the Rust API server registers, with parameters read as wildcards."""
    routes = set()
    for name in sorted(os.listdir(API_DIR)):
        if not name.endswith(".rs"):
            continue
        text = open(os.path.join(API_DIR, name), encoding="utf-8").read()
        for found in ROUTE.finditer(text):
            routes.add(_wildcard(found.group(1)))
        for found in PREFIXED.finditer(text):
            lead, suffix = found.group(1) or "", _wildcard(found.group(2))
            for mount in MOUNTS:
                routes.add(mount + suffix)
            if lead:
                routes.add(_wildcard(lead) + suffix)
    return routes


def _wildcard(path):
    return re.sub(r":[A-Za-z_]+", "{}", path)


def declared(service):
    """The fixed paths a service publishes, without its own parameters."""
    main = os.path.join(REPO, service, "main.py")
    if not os.path.isfile(main):
        return []
    text = open(main, encoding="utf-8").read()
    paths = [found.group(1) for found in ENDPOINT.finditer(text)]
    return [path for path in paths if "{" not in path and path.rstrip("/") not in ("", "/health")]


# Kept on purpose, not for want of routes: these directories are the Python side of a parity test that compares the old
# service with the Rust engine. A service answered by the engine still leaves a useful oracle behind, so deleting it
# would delete the only thing that can prove the engine agrees. The survey says so instead of nagging.
PARITY_ORACLES = {"treasury-analytics-service"}


def verdict(service, gateway_path, routes):
    fixed = declared(service)
    if not fixed:
        return {"service": service, "verdict": "no endpoints", "fixed": 0, "missing": []}
    prefix = gateway_path.rstrip("/")

    def answered(path):
        """A path is answered if the engine registers it, with or without a trailing slash.

        The slash has to be tolerated on both sides: a service publishes "/donations/" and the engine may register
        either form, and comparing a stripped service path against an unstripped route table used to report the whole
        npo surface as missing while every one of its routes was registered.
        """
        candidates = {(prefix + path), path}
        candidates |= {candidate.rstrip("/") for candidate in candidates}
        candidates |= {candidate + "/" for candidate in candidates}
        return candidates & routes

    missing = [path for path in fixed if not answered(path)]
    flat_only = all(path.rstrip("/") in routes for path in fixed) and not all(
        (prefix + path).rstrip("/") in routes for path in fixed
    )
    if service in PARITY_ORACLES:
        return {"service": service, "verdict": "parity oracle", "fixed": len(fixed), "missing": [], "prefix": "keep"}
    return {
        "service": service,
        "verdict": "ready" if not missing else "keep",
        "fixed": len(fixed),
        "missing": missing,
        # flat routes mean the gateway entry has to drop its prefix when it is pointed at the engine
        "prefix": "keep" if not flat_only else "strip",
    }


def survey(wanted_domains):
    owners = manifest().OWNERS
    table = json.load(open(REGISTRY))["services"]
    routes = api_routes()
    rows = []
    for entry in table:
        name = entry["name"]
        domain = owners.get(name, "(unowned)")
        if wanted_domains and domain not in wanted_domains:
            continue
        if not os.path.isdir(os.path.join(REPO, name)):
            continue  # already retired
        rows.append(verdict(name, entry["path"], routes) | {"domain": domain})
    return _without_shared_stripped_paths(rows, table)


def _without_shared_stripped_paths(rows, table):
    # read back through git: a retired service's main.py is only in history by the time this runs
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from gateway_addresses import published, shape

    """A service cannot be called ready when its stripped path is another engine-served service's path.

    The shape test asks whether the engine answers a fixed path. With one engine that is only half the question: it
    answers the address exactly once, so if two stripped services publish `/summary/{company}` the second one's callers
    read the first one's handler and get a 200 with the wrong figures. A ready service therefore also has to own every
    path it would strip down to, and a service that shares one keeps its prefix instead.
    """
    already = {}  # paths claimed by services the engine already answers
    for entry in table:
        if not entry.get("strip_prefix"):
            continue
        if os.path.isdir(os.path.join(REPO, entry["name"])):
            continue
        for path in {shape(p) for p in published(entry["name"])}:
            already.setdefault(path, set()).add(entry["name"])
    claims = {}
    for row in rows:
        if row["verdict"] == "ready":
            for path in {shape(p) for p in published(row["service"])}:
                claims.setdefault(path, set()).add(row["service"])
    for row in rows:
        if row["verdict"] != "ready":
            continue
        shared = set()
        for path in {shape(p) for p in published(row["service"])}:
            others = (claims.get(path, set()) - {row["service"]}) | already.get(path, set())
            if others:
                shared.add("%s with %s" % (path, ", ".join(sorted(others))))
        if shared:
            row["verdict"] = "keep"
            row["missing"] = sorted(shared)
            row["prefix"] = "keep"
    return rows


def report(rows):
    ready = [row for row in rows if row["verdict"] == "ready"]
    keep = [row for row in rows if row["verdict"] == "keep"]
    print("%-46s %-16s %-6s %-6s %s" % ("service", "domain", "fixed", "prefix", "verdict"))
    for row in sorted(rows, key=lambda row: (row["verdict"], row["domain"], row["service"])):
        print(
            "%-46s %-16s %-6d %-6s %s"
            % (row["service"], row["domain"], row["fixed"], row.get("prefix", "-"), row["verdict"])
        )
    print()
    print("%d ready to retire, %d still to keep" % (len(ready), len(keep)))
    for row in keep:
        print("  %s is missing: %s" % (row["service"], ", ".join(row["missing"][:4])))
    by_domain = {}
    for row in ready:
        by_domain.setdefault(row["domain"], []).append(row["service"])
    for domain, services in sorted(by_domain.items()):
        print("  %s: %d ready - %s" % (domain, len(services), " ".join(sorted(services))))
    return len(ready)


if __name__ == "__main__":
    sys.exit(0 if report(survey(set(sys.argv[1:]))) else 1)
