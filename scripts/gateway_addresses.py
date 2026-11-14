"""Does the engine answer every address each retired service still publishes?

A service that has been retired keeps its address: the gateway entry survives, pointed at the engine. That only works if
the engine registers the address the gateway will forward. Whether the gateway forwards the service's prefix or strips it
is per entry, so the two have to agree, and nothing checked that until now.

Run it as a test: tests/test_gateway_addresses.py calls `audit()` and expects no findings.
"""
import json, os, re, subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVICES = os.path.join(ROOT, "gateway", "go", "config", "services.json")
API = os.path.join(ROOT, "rust", "apps", "api-server", "src")
ENDPOINT = re.compile(r'@app\.(?:get|post|put|delete|patch)\(\s*[\'"]([^\'"]+)[\'"]')
ROUTE = re.compile(r'\.route\(\s*"([^"]+)"')
MOUNTS = ("/accounting", "/records/accounting")
# a router that hangs one table off several prefixes declares them in a loop over literal mounts
MOUNT_LOOP = re.compile(r'for\s+\w+\s+in\s*\[([^\]]+)\]')
SKIP = {"", "/", "/health"}

# Addresses a retired service published that the engine has no answer for, each with the reason. The test checks that
# every one of these is still a real gap, so a stale entry cannot hide a working route behind a comment.
KNOWN_GAPS: dict = {}
# The last address here was banking-integration's per-connection feed, and the engine answers it now: the connection
# owns its feed as well as its sweep, so a statement seen over CBZ is not mixed in with one seen over Steward.


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


def published_methods(service):
    """Every (method, path) a retired service published, read back out of git.

    Shapes alone are not proof. A route that answers the right address with the wrong method - a listing published as
    GET while the engine binds only POST - passes an address check and still fails the caller, so the method is kept.
    """
    blob = last_main(service)
    if not blob:
        return set()
    pairs = set()
    for method, path in re.findall(
        r'@app\.(get|post|put|delete|patch)\(\s*[\'"]([^\'"]+)[\'"]', blob
    ):
        pairs.add((method, shape(path)))
    return pairs


def engine_method_routes():
    """The engine's routes as (method, path) pairs, read out of the API server's source.

    A route's path is either a literal or a `format!` with the mount's prefix spliced in, and the methods are the calls
    wrapped around the handler - `post(create).get(list)` is both. Both forms are read the same way: take the arguments
    of the `.route(` call, then read the path and the methods out of them.
    """
    seen = set()

    def route_args(text, open_at):
        depth = 0
        for index in range(open_at, len(text)):
            if text[index] == "(":
                depth += 1
            elif text[index] == ")":
                depth -= 1
                if depth == 0:
                    return text[open_at + 1 : index]
        return ""

    for name in sorted(os.listdir(API)):
        if not name.endswith(".rs"):
            continue
        text = open(os.path.join(API, name), encoding="utf-8").read()
        mounts = MOUNTS
        loops = MOUNT_LOOP.findall(text)
        if loops:
            # this file says which prefixes it answers, so its format!(...) routes hang off those
            # rather than off the accounting mounts. Without it a hand-built table reads as unowned.
            mounts = tuple(re.findall(r'"([^"]+)"', loops[0]))
        for found in re.finditer(r"\.route\(", text):
            args = route_args(text, found.end() - 1)
            if not args:
                continue
            methods = set(re.findall(r"\b(get|post|put|delete|patch)\(", args))
            # published("name") serves a calculator at the address its Python service published, and it takes POST only
            if "published(" in args:
                methods.add("post")
            literal = re.match(r'\s*"([^"]+)"', args)
            if literal:
                seen.update((method, shape(literal.group(1))) for method in methods)
                continue
            mounted = re.match(r'\s*&?format!\(\s*"(?:\{prefix\}|([^"]*))([^"]*)"', args)
            if mounted:
                lead, suffix = mounted.group(1) or "", shape(mounted.group(2))
                if lead:
                    seen.update((method, shape(lead + suffix)) for method in methods)
                else:
                    for mount in mounts:
                        seen.update((method, shape(mount + suffix)) for method in methods)
    return seen

# A published method the engine binds differently or not at all. Shapes alone pass; a caller gets 405 or the wrong
# answer, so each of these is listed with what it would take to close it.
KNOWN_METHOD_GAPS: dict = {}
# Every published method is now answered. The three that were listed here were closed together: banking-integration's
# per-connection feed (GET /transactions/{id}) and its sweep (POST /sync/{id}), and personal finance's in-place
# recurring update (PATCH /recurring/{id}), which an app uses to record a rent increase on the item it already has.


def method_gaps():
    """Published method/path pairs the engine does not answer, ignoring the ones listed as known."""
    engine = engine_method_routes()
    table = json.load(open(SERVICES))
    findings = []
    for entry in table["services"]:
        if os.path.isdir(os.path.join(ROOT, entry["name"])):
            continue
        prefix = entry["path"].rstrip("/")
        for method, path in sorted(published_methods(entry["name"])):
            if path.rstrip("/") in SKIP:
                continue
            if (entry["name"], method, path) in KNOWN_METHOD_GAPS:
                continue
            if any((method, shape(c)) in engine for c in {prefix + path, path}):
                continue
            findings.append(f"{entry['name']}: {method.upper()} {path} reaches nothing in the engine")
    return findings

def stripped_collisions():
    """Paths two engine-served services both claim once their gateway prefix is stripped.

    With one engine holding every route, a stripped path is a single address: if two services both publish `/stats` and
    both are stripped, one of them silently reads the other's handler. Requests still answer, which is why this needs
    checking rather than trusting a green run. A service still on disk runs as its own backend, so its prefix may collide
    freely; only engine-served services are compared.
    """
    table = json.load(open(SERVICES))
    owners = {}
    for entry in table["services"]:
        if not entry.get("strip_prefix") or os.path.isdir(os.path.join(ROOT, entry["name"])):
            continue
        for path in {shape(p) for p in published(entry["name"])}:
            owners.setdefault(path, set()).add(entry["name"])
    return {path: sorted(names) for path, names in owners.items() if len(names) > 1}


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
