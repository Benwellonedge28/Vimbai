"""The generated service status index must stay true.

SERVICE-STATUS.md and the per-service STATUS.md markers are derived from the
gateway route table and the retirement families in test_service_dedup.py. If
someone registers a service, retires one, or deduplicates one without rerunning
scripts/service_status.py, this suite fails and says so.
"""

import importlib.util
import os
import subprocess
import sys

import pytest

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_script = os.path.join(_ROOT, "scripts", "service_status.py")


def _load_script():
    spec = importlib.util.spec_from_file_location("service_status", _script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture(scope="module")
def status():
    return _load_script()


def test_index_is_current():
    result = subprocess.run([sys.executable, _script, "--check"], cwd=_ROOT, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr


def test_index_file_exists():
    assert os.path.isfile(os.path.join(_ROOT, "SERVICE-STATUS.md"))


# Unrouted and not part of any retirement family. Pinned so the list can only
# shrink on purpose: a newly added service has to be registered (or retired)
# rather than quietly joining the grey zone.
# De-registered once the calculation behind them was proven in Rust: the files stay as
# parity oracles, but nothing routes to them any more.
UNDECIDED = {
    "corporate-tax-service",
    "tax-provision-service",
    "tax-risk-service",
    "group-tax-service",
    "vat-reporting-service",
    "r-and-d-tax-service",
    "government-grants-service",
    "internal-control-evaluation-service",
    "internal-controls-testing-service",
    "substantive-testing-service",
    "goodwill-service",
    "finance-service",
    "fraud-detection-service",
    "identity-service",
    "supply-chain-service",
}


def test_every_service_dir_appears_in_the_index(status):
    text = open(os.path.join(_ROOT, "SERVICE-STATUS.md"), encoding="utf-8").read()
    routes = status._gateway_routes()
    for service in status._service_dirs(routes):
        assert f"`{service}`" in text, f"{service} is missing from SERVICE-STATUS.md"


def test_the_undecided_set_only_shrinks(status):
    _, _, unclassified = status.classify()
    assert set(unclassified) <= UNDECIDED, f"new unrouted services: {sorted(set(unclassified) - UNDECIDED)}"


def test_retained_services_carry_a_marker(status):
    live, retained, _ = status.classify()
    assert retained, "expected retained services"
    for service in retained:
        marker = os.path.join(_ROOT, service, "STATUS.md")
        assert os.path.isfile(marker), f"{service} is missing its STATUS.md marker"
        text = open(marker, encoding="utf-8").read()
        assert "Do not delete this directory" in text


def test_live_services_do_not_carry_a_stale_marker(status):
    live, _, _ = status.classify()
    for service in live:
        assert not os.path.isfile(os.path.join(_ROOT, service, "STATUS.md"))


def test_ported_calculators_name_their_crate(status):
    _, retained, _ = status.classify()
    for service, info in retained.items():
        if info["family"].endswith("DUPLICATES"):
            continue
        assert info["crate"], f"{service} should name the Rust crate that owns its figures"
        crate_dir = os.path.join(_ROOT, "library", info["crate"])
        # ledger-core predates the library workspace and sits at the root.
        assert os.path.isdir(crate_dir) or os.path.isdir(
            os.path.join(_ROOT, info["crate"])
        ), f"{info['crate']} is not a Rust crate"


def test_index_records_the_running_product(status):
    live, retained, unclassified = status.classify()
    text = open(os.path.join(_ROOT, "SERVICE-STATUS.md"), encoding="utf-8").read()
    assert f"live and routed          {len(live)}" in text
    assert f"retained, not routed     {len(retained)}" in text
    assert f"unrouted, undecided      {len(unclassified)}" in text
    for service in list(live)[:5] + list(retained)[:5] + unclassified:
        assert f"`{service}`" in text
