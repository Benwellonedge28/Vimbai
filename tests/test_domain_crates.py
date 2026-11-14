"""Every domain owns one crate, and the store owns none of the domains' work.

The services used to be separate Python containers that each carried a copy of the same models,
the same database access and the same arithmetic. Porting them one at a time into the shared store
crate would have repeated that mistake in Rust: fourteen modules side by side in the adapter, each
reachable only through it. They now live in one crate per business domain, and this test is what
keeps it that way.
"""

import os
import re

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CATALOG = os.path.join(ROOT, "catalog", "services", "catalog.yaml")
STORE = os.path.join(ROOT, "rust", "crates", "adapters", "database", "src")

# The store's own work: the record shapes, the Book scoping, the driver, and the helpers their
# tests use. Nothing here computes a business figure.
STORE_MODULES = {
    "lib.rs",
    "amounts.rs",
    "fake.rs",
    "neo4j_client.rs",
    # machinery, not a service: reading a value out of a query string, shared by every port that has
    # to honour ?limit=5 or ?reconciled=true instead of silently keeping its default
    "query.rs",
    # machinery too: reading a value out of a request payload and writing a record back, shared by
    # every port that used to carry its own copy of them
    "read.rs",
    "write.rs",
    # machinery as well: the one keying that is not per person, for records whose owner is the Book rather than a
    # person. A shared Book's members are different people reading one another's rows, and the rows still belong to a
    # service; what lives here is only how they are found and stored, gated by the membership check the caller makes.
    "shared.rs",
    # machinery too: the store over a file rather than a server, so the on-device runtime can hold a Book without a
    # database. It knows the record statements and nothing about what the records mean.
    "file.rs",
}


def catalog_services():
    with open(CATALOG, encoding="utf-8") as handle:
        return yaml.safe_load(handle)["services"]


def rust_services():
    return [row for row in catalog_services() if row.get("engine") == "rust"]


def target_dir(target):
    """`rust/crates/domains/costing` -> the absolute path, so the test can check it exists."""
    return os.path.join(ROOT, *target.split("/"))


def test_the_store_holds_no_service_module():
    present = {name for name in os.listdir(STORE) if name.endswith(".rs")}
    assert present == STORE_MODULES, (
        "the store grew a service module again: "
        f"{sorted(present - STORE_MODULES)}. A service belongs in its domain crate."
    )


def test_the_store_declares_no_service_module():
    with open(os.path.join(STORE, "lib.rs"), encoding="utf-8") as handle:
        text = handle.read()
    declared = set(re.findall(r"^pub mod (\w+);", text, re.MULTILINE))
    machinery = {"amounts", "fake", "neo4j_client", "query", "read", "shared", "write", "file"}
    assert declared <= machinery, (
        f"the store still re-exports a domain module: {sorted(declared - machinery)}"
    )


def test_every_rust_service_has_a_crate_to_live_in():
    missing = []
    for row in rust_services():
        directory = target_dir(row["target"])
        if not os.path.isfile(os.path.join(directory, "Cargo.toml")):
            missing.append(f"{row['name']} -> {row['target']}")
    assert missing == [], f"a Rust service has no crate: {missing}"


def test_every_rust_service_is_named_by_its_domain_crate():
    """A service the catalogue calls Rust must be listed by the crate that houses it."""
    unnamed = []
    for row in rust_services():
        lib = os.path.join(target_dir(row["target"]), "src", "lib.rs")
        with open(lib, encoding="utf-8") as handle:
            text = handle.read()
        if row["name"] not in text:
            unnamed.append(f"{row['name']} not named in {row['target']}/src/lib.rs")
    assert unnamed == [], unnamed


def test_every_crate_is_safe_rust_only():
    """Every crate in the workspace, not only the domains.

    This used to sweep `crates/domains/` alone, and the sweep was what made the rule look kept: the kernel crate,
    which is the one crate where an unsafe block would matter most, carried no attribute at all and no test noticed.
    Kernel, platform, calculations, application, adapters and the applications are all covered here.
    """
    unsafe_gaps = []
    for directory, _, files in os.walk(os.path.join(ROOT, "rust")):
        if "target" in directory.split(os.sep) or "Cargo.toml" not in files:
            continue
        entry = os.path.join(directory, "src", "main.rs")
        if not os.path.isfile(entry):
            entry = os.path.join(directory, "src", "lib.rs")
        if not os.path.isfile(entry):
            continue
        with open(entry, encoding="utf-8") as handle:
            text = handle.read()
        if "#![forbid(unsafe_code)]" not in text:
            unsafe_gaps.append(os.path.relpath(directory, os.path.join(ROOT, "rust")))
    assert unsafe_gaps == [], f"crates without forbid(unsafe_code): {sorted(unsafe_gaps)}"


def test_a_domain_crate_holds_every_service_of_that_domain_together():
    """One crate per domain: no domain may be split across two crates."""
    by_domain = {}
    for row in rust_services():
        by_domain.setdefault(row["domain"], set()).add(row["target"])
    split = {domain: sorted(targets) for domain, targets in by_domain.items() if len(targets) > 1}
    assert split == {}, f"a domain's services live in more than one crate: {split}"


def test_the_costing_crate_carries_all_of_the_costing_services():
    """The cost accounting services are the ones this consolidation was done for."""
    costing = "rust/crates/domains/costing"
    expected = {name for name in (row["name"] for row in rust_services() if row["domain"] == "costing")}
    with open(os.path.join(target_dir(costing), "src", "lib.rs"), encoding="utf-8") as handle:
        text = handle.read()
    for name in expected:
        assert name in text, f"{name} is not named by the costing crate"
    assert len(expected) >= 7, f"expected the costing services, found {sorted(expected)}"
