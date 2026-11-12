#!/usr/bin/env python3
"""Which Rust modules can anything actually reach?

Four times now a calculation has been written in Rust and then never given a door: the float CVP,
basis apportionment, process costing, and - found by this script - twenty more modules including
IFRS 15 revenue, IFRS 2 share-based payment, IFRS 9 instruments, business valuation and time value of
money. Every one of them looked wired, because every one of them is `pub` and re-exported.

A module is reachable when it is imported, directly or through another module, by one of the three
doors the platform has: the API server, the native extension, or the bridge both of them share.
Nothing else can call into the library, so anything not reached is arithmetic that no request and no
Python service can execute.

This is a static reading of the imports, not the compiler, so it proves what is *not* reachable more
reliably than what is: a module it reports as dead has no import path from any door.
"""

from __future__ import annotations

import collections
import glob
import os
import re
import sys

DOORS = ("rust/apps", "rust/native", "rust/crates/adapters")


def _crate_dir(path: str) -> str:
    """The nearest directory at or above the file that holds a Cargo.toml."""
    directory = os.path.dirname(os.path.abspath(path))
    while directory != os.path.dirname(directory):
        if os.path.exists(os.path.join(directory, "Cargo.toml")):
            return os.path.relpath(directory, os.getcwd()).replace(os.sep, "/")
        directory = os.path.dirname(directory)
    return "/".join(os.path.abspath(path).split(os.sep)[:2])


def _crate_name(directory: str) -> str:
    manifest = os.path.join(directory, "Cargo.toml")
    if os.path.exists(manifest):
        with open(manifest, encoding="utf-8") as handle:
            match = re.search(r'^name = "([^"]+)"', handle.read(), re.M)
        if match:
            return match.group(1)
    return os.path.basename(directory)


def _package_at(directory: str) -> str:
    manifest = os.path.join(directory, "Cargo.toml")
    if os.path.exists(manifest):
        with open(manifest, encoding="utf-8") as handle:
            match = re.search(r'^name = "([^"]+)"', handle.read(), re.M)
        if match:
            return match.group(1)
    return ""


def _renames(directory: str, packages: dict[str, str]) -> dict[str, str]:
    """Dependency name -> package name, for every dependency a crate declares.

    Plenty of the workspace imports a crate by a shorter path than its package: `use assets::{
    depreciation}` names the directory, while the package is `vimbai_assets`. An import graph built
    only from `vimbai_*` names reports exactly the wired crates as dead, which is the mistake this
    function exists to prevent.
    """
    manifest = os.path.join(directory, "Cargo.toml")
    if not os.path.exists(manifest):
        return {}
    aliases: dict[str, str] = {}
    in_deps = False
    with open(manifest, encoding="utf-8") as handle:
        for line in handle:
            stripped = line.split("#", 1)[0].strip()
            if stripped.startswith("["):
                section = stripped.strip("[]")
                in_deps = section in ("dependencies", "dev-dependencies", "build-dependencies")
                in_deps = in_deps or section.startswith("dependencies.")
                continue
            match = re.match(r"^([A-Za-z_][A-Za-z0-9_\-]*)\s*=", stripped)
            if not (in_deps and match):
                continue
            key = match.group(1)
            package = re.search(r'package\s*=\s*"([^"]+)"', stripped)
            if package:
                aliases[key.replace("-", "_")] = package.group(1)
                continue
            target = re.search(r'path\s*=\s*"([^"]+)"', stripped)
            if target:
                resolved = packages.get(os.path.normpath(os.path.join(directory, target.group(1))).replace(os.sep, "/"))
                if resolved:
                    aliases[key.replace("-", "_")] = resolved
                    continue
            aliases[key.replace("-", "_")] = key
    return aliases


def _module_path(path: str, directory: str) -> tuple[str, ...]:
    relative = os.path.relpath(path, os.path.join(directory, "src"))[:-3]
    return () if relative == "lib" else tuple(relative.split(os.sep))


def build(root: str = "rust"):
    """Return (node map, crate map, import edges) for every .rs file under `root`."""
    files = [
        path
        for path in glob.glob(os.path.join(root, "**", "*.rs"), recursive=True)
        if os.sep + "target" + os.sep not in path
    ]
    crates = {path: _crate_name(_crate_dir(path)) for path in files}
    nodes = {path: (crates[path], _module_path(path, _crate_dir(path))) for path in files}

    packages = {}
    for manifest in glob.glob(os.path.join(root, "**", "Cargo.toml"), recursive=True):
        packages[os.path.dirname(manifest).replace(os.sep, "/")] = _package_at(os.path.dirname(manifest))
    aliases = {path: _renames(_crate_dir(path), packages) for path in files}
    edges: dict[tuple[str, tuple[str, ...]], set] = collections.defaultdict(set)
    for path, node in nodes.items():
        with open(path, encoding="utf-8") as handle:
            source = handle.read()
        for alias, package in aliases[path].items():
            for match in re.finditer(rf"\b{re.escape(alias)}::([a-z_0-9]+(?:::[a-z_0-9]+)*)", source):
                edges[node].add((package, tuple(match.group(1).split("::"))))
            # `use treasury::{cash_optimization, treasury_analytics};` - a brace group hides every
            # module behind it, and that is how the routed treasury maths is imported.
            for group in re.finditer(rf"\b{re.escape(alias)}::\{{([^}}]*)\}}", source):
                for item in group.group(1).split(","):
                    head = item.strip().split("::")[0].strip()
                    if head and head != "self":
                        edges[node].add((package, (head,)))
        for match in re.finditer(r"crate::([a-z_0-9]+(?:::[a-z_0-9]+)*)", source):
            edges[node].add((node[0], tuple(match.group(1).split("::"))))
        for match in re.finditer(r"super::([a-z_0-9]+)", source):
            parent = node[1][: max(len(node[1]) - 1, 0)]
            edges[node].add((node[0], parent + (match.group(1),)))
        for match in re.finditer(r"\bvimbai_([a-z_]+)::([a-z_0-9]+(?:::[a-z_0-9]+)*)", source):
            edges[node].add(("vimbai_" + match.group(1), tuple(match.group(2).split("::"))))
    return files, crates, nodes, edges


def reachable(root: str = "rust") -> set[str]:
    """The .rs files a door can reach, following imports through as many crates as it takes."""
    files, crates, nodes, edges = build(root)
    by_node: dict[tuple[str, tuple[str, ...]], list[str]] = collections.defaultdict(list)
    for path, node in nodes.items():
        by_node[node].append(path)

    # A door often calls `assets::depreciation_batch_from_json`, a function re-exported by the crate
    # root, not a module path. Resolving those names to the module that defines them is the
    # difference between "dead" and "wired by a re-export".
    defines: dict[tuple[str, str], tuple[str, ...]] = {}
    for path, (crate, module) in nodes.items():
        with open(path, encoding="utf-8") as handle:
            for name in re.findall(r"^pub fn ([a-z0-9_]+)", handle.read(), re.M):
                defines.setdefault((crate, name), module)

    queue = [nodes[path] for path in files if path.startswith(tuple(DOORS))]
    seen: set[tuple[str, tuple[str, ...]]] = set()
    while queue:
        node = queue.pop()
        if node in seen:
            continue
        seen.add(node)
        for crate, module in edges.get(node, ()):
            roots = {m[0] for c, m in by_node if c == crate and m}
            if module and module[0] not in roots:
                resolved = defines.get((crate, module[0]))
                if resolved:
                    module = resolved
            queue.extend(
                candidate
                for candidate in by_node
                if candidate[0] == crate and module[: len(candidate[1])] == candidate[1]
            )
    return {path for node in seen for path in by_node[node]}


def unreachable_modules(root: str = "rust") -> list[str]:
    """Module files no door reaches, as "<crate>::<path>" strings, sorted."""
    files, crates, nodes, _ = build(root)
    reached = reachable(root)
    dead = []
    for path in files:
        if path in reached or not path.startswith(os.path.join(root, "crates")):
            continue
        if path.endswith("lib.rs") or "/tests" in path or path.endswith("tests.rs"):
            continue
        module = "::".join(nodes[path][1]) or "lib"
        dead.append(f"{crates[path].replace('vimbai_', '')}::{module}")
    return sorted(set(dead))


if __name__ == "__main__":
    for name in unreachable_modules():
        print(name)
    sys.exit(0)
