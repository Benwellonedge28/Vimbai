"""Nothing is wired twice, and nothing is written twice.

Duplication in this repository has not looked like copy-paste. It has looked like
two things answering to one name and the second silently winning:

* `tax-provision` was registered as a calculator twice, once through the Python
  seam and once through the tax crate, and Rust's `BTreeMap` threw the first one
  away without a word. The seam entry was dead and nobody could tell by reading.
* `py_round` existed in two crates with two different algorithms - a formatter in
  one, hand-rolled banker's rounding with a tolerance in the other - so a fix to
  one was not a fix to the other.
* the same cost-accounting maths was written once in `domains/costing` and once
  in `calculations/cost_accounting`, in exact arithmetic and in floats.

Each check here is cheap and catches one of those shapes.
"""

import collections
import glob
import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
API = os.path.join(ROOT, "rust", "apps", "api-server", "src", "lib.rs")
ROUTES = os.path.join(ROOT, "gateway", "go", "config", "services.json")

# Cross-crate same-name, same-arity functions that are genuinely different
# calculations rather than copies: `calculate`, `assess`, `evaluate` and friends
# are each crate's entry point for its own request type. A new one of these is a
# design decision, so the set is pinned and may not grow quietly.
ALLOWED_SHARED_NAMES = {
    ("acquire", 1),
    ("analyze", 1),
    ("assess", 1),
    ("calculate", 1),
    ("calculate_from_json", 1),
    ("consolidate", 1),
    ("evaluate", 1),
    ("generate", 1),
    ("prepare", 1),
    ("prepare_from_json", 1),
    ("test", 1),
    ("validate", 1),
}


def calculator_registrations():
    with open(API, encoding="utf-8") as handle:
        source = handle.read()
    return re.findall(r'map\.insert\(\s*\n?\s*"([a-z0-9\-]+)"', source)


def test_no_calculator_is_registered_twice():
    names = calculator_registrations()
    duplicate = sorted(name for name, count in collections.Counter(names).items() if count > 1)
    assert not duplicate, (
        "these calculators are registered more than once, so the earlier registration is dead code "
        f"that looks alive: {duplicate}"
    )


def test_no_gateway_path_is_routed_twice():
    with open(ROUTES, encoding="utf-8") as handle:
        raw = handle.read()
    paths = re.findall(r'"path"\s*:\s*"([^"]+)"', raw)
    duplicate = sorted(path for path, count in collections.Counter(paths).items() if count > 1)
    assert not duplicate, f"two services answer the same path: {duplicate}"


def test_no_service_name_appears_twice_in_the_route_table():
    with open(ROUTES, encoding="utf-8") as handle:
        document = json.load(handle)
    names = []

    def walk(node):
        if isinstance(node, dict):
            for key, value in node.items():
                if key in ("name", "service", "target") and isinstance(value, str):
                    names.append(value)
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(document)
    duplicate = sorted(name for name, count in collections.Counter(names).items() if count > 1)
    assert not duplicate, f"the route table names these services more than once: {duplicate}"


def test_no_two_source_files_are_identical():
    """A byte-identical file in two crates is a copy waiting to drift."""
    seen = {}
    duplicate = []
    for path in glob.glob(os.path.join(ROOT, "rust", "crates", "**", "*.rs"), recursive=True):
        if "/target/" in path:
            continue
        with open(path, "rb") as handle:
            body = handle.read()
        if len(body) < 400:  # a re-export module is allowed to look like another
            continue
        if body in seen:
            duplicate.append((seen[body], path))
        else:
            seen[body] = path
    assert not duplicate, "identical source files, so one is a copy: " + ", ".join(
        f"{os.path.relpath(a, ROOT)} = {os.path.relpath(b, ROOT)}" for a, b in duplicate[:5]
    )


# Modules with the same file name in two crates that share a public function name. Each of these
# has been looked at and is a different calculation behind a familiar word, not a copy:
#   job_costing  the seam's f64 adapter over domains/costing's exact `settle`, which is the one rule
#   consolidation  a group's consolidation statements versus a business combination's own
#   xbrl  two live generators with different shapes, both reachable through their own service
PINNED_SAME_MODULE_CLASHES = {"job_costing", "consolidation", "xbrl"}


def test_no_new_module_name_clash_between_crates():
    """Three of these survived, and each cost CVP, apportionment and job costing. A fourth is news."""
    import collections as _collections

    modules = _collections.defaultdict(list)
    for path in glob.glob(os.path.join(ROOT, "rust", "crates", "**", "src", "*.rs"), recursive=True):
        if "/target/" in path:
            continue
        stem = os.path.basename(path)[:-3]
        if stem in ("lib", "mod"):
            continue
        with open(path, encoding="utf-8") as handle:
            source = handle.read()
        modules[stem].append((path, set(re.findall(r"^pub fn ([a-z0-9_]+)", source, re.M))))

    clashes = set()
    for stem, entries in modules.items():
        for index, (path_a, fns_a) in enumerate(entries):
            for path_b, fns_b in entries[index + 1 :]:
                if fns_a & fns_b and os.path.dirname(os.path.dirname(path_a)) != os.path.dirname(
                    os.path.dirname(path_b)
                ):
                    clashes.add(stem)
    unexpected = sorted(clashes - PINNED_SAME_MODULE_CLASHES)
    assert not unexpected, (
        "a module name now exists in two crates with the same public function names, which is how "
        f"the CVP, apportionment and process-costing duplicates started: {unexpected}"
    )


def test_no_new_shared_function_name_across_crates():
    signatures = collections.defaultdict(list)
    for path in glob.glob(os.path.join(ROOT, "rust", "crates", "**", "*.rs"), recursive=True):
        if "/tests" in path or path.endswith("tests.rs") or "/target/" in path:
            continue
        with open(path, encoding="utf-8") as handle:
            source = handle.read()
        for match in re.finditer(r"^pub fn ([a-z0-9_]+)\(([^)]*)\)", source, re.M):
            name, args = match.group(1), match.group(2)
            arity = 1 if args.strip() in ("", "&self", "&mut self") else args.count(",") + 1
            signatures[(name, arity)].append(os.path.relpath(path, ROOT))

    def is_shim(name, file):
        """A wrapper that hands straight to another crate's function is fine - one definition,
        one name, no second algorithm to drift."""
        with open(os.path.join(ROOT, file), encoding="utf-8") as handle:
            body = handle.read()
        return f"::{name}(" in body

    shared = {
        key
        for key, files in signatures.items()
        if len({os.path.dirname(f) for f in files}) > 1 and not any(is_shim(key[0], f) for f in files)
    }
    unexpected = sorted(shared - ALLOWED_SHARED_NAMES)
    assert not unexpected, (
        "these function names now exist in more than one crate, which is how the duplicate "
        f"implementations started: {unexpected}. Share one definition instead."
    )
