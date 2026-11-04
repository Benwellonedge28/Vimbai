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
SEAM_TEST = ROOT / "tests" / "test_assurance_seam.py"
TRACKER = ROOT / "library" / "CALCULATOR-PORT.md"
API_REGISTRY = ROOT / "library" / "api" / "src" / "lib.rs"


def extension_functions():
    try:
        import vimbai_native  # noqa: F401
    except ImportError:
        return None
    return {name for name in dir(vimbai_native) if name.endswith("_json")}


def services_with_calculator():
    found = {}
    for calc in ROOT.glob("*/calc.py"):
        text = calc.read_text()
        names = re.findall(r'"(\w+_json)"', text) or re.findall(r'SEAM = "(\w+_json)"', text)
        if names:
            found[calc.parent.name] = sorted(set(names))
    return found


def api_names():
    if not API_REGISTRY.exists():
        return set()
    return set(re.findall(r'map\.insert\(\s*"([\w-]+)"', API_REGISTRY.read_text()))


def parity_tested():
    return SEAM_TEST.read_text() if SEAM_TEST.exists() else ""


def main():
    functions = extension_functions()
    wired = services_with_calculator()
    served = api_names()
    parity = parity_tested()

    rows = []
    for service, seams in sorted(wired.items()):
        bound = functions is not None and all(name in functions for name in seams)
        tested = service in parity
        http = service.removesuffix("-service") in served
        rows.append((service, seams, bound, tested, http))

    print(f"{'service':48s} {'seam bound':10s} {'parity':7s} {'http':5s} verdict")
    ready = []
    for service, seams, bound, tested, http in rows:
        verdict = "delete-safe" if (bound and tested and http) else "keep"
        if verdict == "delete-safe":
            ready.append(service)
        print(
            f"{service:48s} {str(bound):10s} {str(tested):7s} {str(http):5s} {verdict}"
        )

    print()
    print(f"{len(rows)} services answer through the seam")
    print(f"{len(ready)} pass every check and may be retired: {', '.join(ready) or 'none yet'}")
    if functions is None:
        print("the extension is not installed here, so 'seam bound' could not be checked")
    return 0 if ready else 1


if __name__ == "__main__":
    sys.exit(main())
