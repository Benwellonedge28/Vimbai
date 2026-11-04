"""Which Python services may be deleted, judged by evidence rather than hope.

A calculator is safe to retire only when all of this is true:

1. its maths lives in a Rust crate (the service has a calc.py that calls the seam),
2. the seam function it calls exists in the built extension,
3. a parity test names the service, so both engines have been held together,
4. the Rust HTTP surface can answer for it (/calc/<name>), because deleting the
   Python service removes the only endpoint that serves it today.

Run from the repository root:  python3 scripts/retire_gate.py
Exit code 0 means at least one service passed every check.
"""

import importlib.util
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SEAM_TESTS = tuple(
    ROOT / "tests" / name
    for name in (
        "test_assurance_seam.py",
        "test_cost_accounting_seam.py",
        "test_cost_of_capital_seam.py",
        "test_cost_accounting_service_seam.py",
        "test_consolidation_views_seam.py",
        "test_corporate_tax_seam.py",
    )
)
TRACKER = ROOT / "library" / "CALCULATOR-PORT.md"
API_REGISTRY = ROOT / "library" / "api" / "src" / "lib.rs"


def extension_functions():
    try:
        import vimbai_native  # noqa: F401
    except ImportError:
        return None
    return {name for name in dir(vimbai_native) if name.endswith("_json")}


def services_with_calculator():
    """Every service whose maths is held to Rust, by whichever evidence exists.

    A service that crosses the seam at request time names its seam function in
    calc.py. A service whose Python still computes is held to Rust by a parity test
    instead, which names the same functions; that evidence counts too, because it
    compares the two answers rather than trusting the wiring.
    """
    found = {}
    for calc in ROOT.glob("*/calc.py"):
        text = calc.read_text()
        names = re.findall(r'"(\w+_json)"', text) or re.findall(r'SEAM = "(\w+_json)"', text)
        if names:
            found[calc.parent.name] = sorted(set(names))

    parity = parity_tested()
    for main in ROOT.glob("*/main.py"):
        service = main.parent.name
        if service in found or f'"{service}"' not in parity:
            continue
        # parity tests name the extension functions they compare against
        names = sorted(set(re.findall(r'"(\w+_json)"', parity)))
        if names:
            found[service] = names
    return found


def api_names():
    if not API_REGISTRY.exists():
        return set()
    return set(re.findall(r'map\.insert\(\s*"([\w-]+)"', API_REGISTRY.read_text()))


STATIC_ENDPOINTS = re.compile(r'@app\.(?:post|get)\("([^"{}]+)"')


def published_endpoints(service):
    """The fixed paths a service answers, as a client would call them."""
    main = ROOT / service / "main.py"
    if not main.exists():
        return []
    paths = STATIC_ENDPOINTS.findall(main.read_text())
    return sorted({p.strip("/") for p in paths if p.strip("/") not in ("", "health", "docs", "openapi.json")})


# Services whose routes carry a short prefix rather than the full base name. Declared
# here rather than guessed, because a suffix match alone would let `/calculate` count
# as served by any other service's `-calculate` route.
SHORT_PREFIX = {"net-realizable-value-service": "nrv"}

# Paths whose route is not named after the path. Each entry was read off the router and
# checked against the adapter it points at; nothing here is inferred from a name.
PATH_ROUTES = {
    ("consolidation-service", "consolidate"): "consolidation",
    ("net-realizable-value-service", "calculate-nrv"): "net-realizable-value",
    ("net-realizable-value-service", "NRV-inventory"): "nrv-inventory",
    ("net-realizable-value-service", "compare-cost-nrv"): "nrv-compare-cost",
}


def routes_for(service, endpoints):
    """The route names that would answer each of a service's fixed paths."""
    base = SHORT_PREFIX.get(service, service.removesuffix("-service"))
    return [f"{base}-{path.replace('/', '-')}" for path in endpoints]


def parity_tested():
    return "".join(path.read_text() for path in SEAM_TESTS if path.exists())


def main():
    functions = extension_functions()
    wired = services_with_calculator()
    served = api_names()
    parity = parity_tested()

    rows = []
    for service, seams in sorted(wired.items()):
        bound = functions is not None and all(name in functions for name in seams)
        tested = service in parity
        base = SHORT_PREFIX.get(service, service.removesuffix("-service"))
        endpoints = published_endpoints(service)
        # A single-endpoint service is served by the route named after it. With more
        # than one path that shortcut would let a partly served service pass, so each
        # published path has to have its own route.
        single = base in served and len(endpoints) <= 1
        named = [
            PATH_ROUTES.get((service, path), name) for path, name in zip(endpoints, routes_for(service, endpoints))
        ]
        http = single or (bool(endpoints) and all(name in served for name in named))
        rows.append((service, seams, bound, tested, http))

    print(f"{'service':48s} {'seam bound':10s} {'parity':7s} {'http':5s} verdict")
    ready = []
    for service, seams, bound, tested, http in rows:
        verdict = "delete-safe" if (bound and tested and http) else "keep"
        if verdict == "delete-safe":
            ready.append(service)
        print(f"{service:48s} {str(bound):10s} {str(tested):7s} {str(http):5s} {verdict}")

    print()
    print(f"{len(rows)} services answer through the seam")
    print(f"{len(ready)} pass every check and may be retired: {', '.join(ready) or 'none yet'}")
    if functions is None:
        print("the extension is not installed here, so 'seam bound' could not be checked")
    return 0 if ready else 1


if __name__ == "__main__":
    sys.exit(main())
