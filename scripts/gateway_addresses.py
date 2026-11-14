"""Does the engine answer every address each retired service still publishes?

A service that has been retired keeps its address: the gateway entry survives, pointed at the engine. That only works if
the engine registers the address the gateway will forward. Whether the gateway forwards the service's prefix or strips it
is per entry, so the two have to agree, and nothing checked that until now.

Run it as a test: tests/test_gateway_addresses.py calls `audit()` and expects no findings.
"""
import json, os, re, subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API = os.path.join(ROOT, "rust", "apps", "api-server", "src")
ENDPOINT = re.compile(r'@app\.(?:get|post|put|delete|patch)\(\s*[\'"]([^\'"]+)[\'"]')
ROUTE = re.compile(r'\.route\(\s*"([^"]+)"')
MOUNTS = ("/accounting", "/records/accounting")
SKIP = {"", "/", "/health"}

# Addresses a retired service published that the engine has no answer for, each with the reason. The test checks that
# every one of these is still a real gap, so a stale entry cannot hide a working route behind a comment.
KNOWN_GAPS = {
    ("banking-integration-service", "/transactions/{connection_id}"): (
        "there is no per-connection transaction feed; statements are read for the whole book through /statements"
    ),
}


def shape(path):
    """A path with its parameters blanked, so {id} and :id compare equal."""
    path = re.sub(r"\{[^}]*\}", "{}", path)
    path = re.sub(r":[A-Za-z_][A-Za-z0-9_]*", "{}", path)
    return path.rstrip("/") or "/"


def engine_routes():
    seen = set()
    for name in sorted(os.listdir(API)):
        if not name.endswith(".rs"):
            continue
        text = open(os.path.join(API, name), encoding="utf-8").read()
        for found in ROUTE.finditer(text):
            seen.add(shape(found.group(1)))
        for found in re.finditer(r'format!\("(?:\{prefix\}|([^"]*))([^"]*)"', text):
            lead, suffix = found.group(1) or "", shape(found.group(2))
            for mount in MOUNTS:
                seen.add(shape(mount + suffix))
            if lead:
                seen.add(shape(lead + suffix))
    return seen


def last_main(service):
    """The last version of a retired service's main.py, read back out of git."""
    commit = subprocess.run(
        ["git", "-C", ROOT, "log", "-1", "--format=%H", "--", service + "/main.py"],
        capture_output=True, text=True,
    ).stdout.strip()
    if not commit:
        return None
    parents = subprocess.run(
        ["git", "-C", ROOT, "rev-list", "--parents", "-n", "1", commit],
        capture_output=True, text=True,
    ).stdout.split()
    if len(parents) < 2:
        return None
    try:
        return subprocess.run(
            ["git", "-C", ROOT, "show", parents[1] + ":" + service + "/main.py"],
            capture_output=True, text=True, check=True,
        ).stdout
    except subprocess.CalledProcessError:
        return None


def published(service):
    text = last_main(service)
    if text is None:
        return None
    return [p for p in ENDPOINT.findall(text) if p.rstrip("/") not in SKIP]


def audit():
    """Every address a retired service published, checked against the form its gateway entry forwards."""
    registry = json.load(open(os.path.join(ROOT, "gateway", "go", "config", "services.json")))["services"]
    engine = engine_routes()
    findings = []
    for entry in registry:
        if "8099" not in entry["url"]:
            continue
        service = entry["name"]
        if os.path.isdir(os.path.join(ROOT, service)):
            continue  # still on disk, so its own app answers
        paths = published(service)
        if not paths:
            continue
        keeps = not entry.get("strip_prefix", False)
        prefix = entry["path"].rstrip("/")
        for path in paths:
            address = prefix + path if keeps else path
            if shape(address) in engine:
                continue
            if (service, path) in KNOWN_GAPS:
                continue
            findings.append(f"{service}: {address} reaches nothing in the engine")
    return findings


def stale_gaps():
    """Known gaps that the engine now answers, so the excuse above has outlived its truth."""
    registry = json.load(open(os.path.join(ROOT, "gateway", "go", "config", "services.json")))["services"]
    engine = engine_routes()
    entries = {entry["name"]: entry for entry in registry}
    stale = []
    for (service, path), _ in KNOWN_GAPS.items():
        entry = entries.get(service)
        if entry is None:
            stale.append(f"{service} is no longer routed")
            continue
        address = entry["path"].rstrip("/") + path if not entry.get("strip_prefix", False) else path
        if shape(address) in engine:
            stale.append(f"{service}: {address} is answered now, so drop it from KNOWN_GAPS")
    return stale


if __name__ == "__main__":
    found = audit()
    print("addresses a retired service publishes that nothing answers:", len(found))
    for line in found:
        print("  " + line)
    for line in stale_gaps():
        print("  stale: " + line)
