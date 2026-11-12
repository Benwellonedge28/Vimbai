"""The migration manifest must stay derived, complete and honest.

Three things are checked: every routed service has exactly one row, every row names a domain that
exists, and the committed document still matches what the generator produces. That last one is what
stops the plan from drifting away from the product.
"""

import importlib.util
import os
import sys

import pytest

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO, "scripts"))


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MANIFEST = _load("migration_manifest", os.path.join(REPO, "scripts", "migration_manifest.py"))


def test_every_routed_service_has_an_owner():
    routed = {service["name"] for service in MANIFEST.routed()}
    unowned = sorted(routed - set(MANIFEST.OWNERS))
    assert not unowned, f"routed services with no owner domain: {unowned}"


def test_no_owner_survives_retirement():
    routed = {service["name"] for service in MANIFEST.routed()}
    stale = sorted(set(MANIFEST.OWNERS) - routed)
    assert not stale, f"owner rows for services the gateway no longer routes: {stale}"


def test_each_row_names_a_real_domain():
    unknown = sorted(set(MANIFEST.OWNERS.values()) - set(MANIFEST.DOMAINS))
    assert not unknown, f"owners point at domains with no destination: {unknown}"


@pytest.mark.parametrize("domain", sorted(MANIFEST.DOMAINS))
def test_each_domain_says_where_its_code_lands(domain):
    purpose, destination = MANIFEST.DOMAINS[domain]
    assert purpose and destination
    assert destination.startswith("rust/crates/")


def test_the_committed_manifest_matches_the_generator():
    with open(MANIFEST.OUT, encoding="utf-8") as handle:
        committed = handle.read()
    assert committed == MANIFEST.render(), "run scripts/migration_manifest.py and commit the result"
