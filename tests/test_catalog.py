"""The catalogue has to agree with the gateway and the manifest, or it is a third opinion."""

import importlib.util
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def _load():
    path = os.path.join(ROOT, "scripts", "generate_catalog.py")
    spec = importlib.util.spec_from_file_location("generate_catalog", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


catalog = _load()


def test_every_routed_service_appears_exactly_once():
    names = [row["name"] for row in catalog.entries()]
    assert len(names) == len(set(names))
    routed = [service["name"] for service in catalog.MANIFEST.routed()]
    assert sorted(names) == sorted(routed)


def test_every_service_has_an_owner_domain():
    unowned = [row["name"] for row in catalog.entries() if not row["domain"]]
    assert unowned == [], f"routed but unowned: {unowned}"


def test_ports_and_paths_match_the_gateway():
    by_name = {service["name"]: service for service in catalog.MANIFEST.routed()}
    for row in catalog.entries():
        assert row["port"] == by_name[row["name"]].get("port")
        assert row["gateway_path"] == by_name[row["name"]].get("path")


def test_the_committed_catalogue_matches_the_generator():
    with open(os.path.join(ROOT, "catalog", "services", "catalog.yaml"), encoding="utf-8") as handle:
        assert handle.read() == catalog.catalog_text()


def test_each_domain_file_matches_the_generator():
    written = catalog.domain_texts()
    directory = os.path.join(ROOT, "catalog", "services")
    on_disk = {name[: -len(".yaml")] for name in os.listdir(directory) if name != "catalog.yaml"}
    assert on_disk == set(written), "a domain file is missing or stale"
    for domain, text in written.items():
        with open(os.path.join(directory, f"{domain}.yaml"), encoding="utf-8") as handle:
            assert handle.read() == text, domain
