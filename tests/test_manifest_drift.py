"""The gateway manifest, the cluster manifests and the directories on disk must agree.

Three retirement batches found services wired into the cluster after they had left the
gateway manifest, and nothing was checking, so a retired service could keep a live
deployment behind it. This pins today's disagreements in k8s/manifest-drift.json: new
drift fails, and each entry is removed from that file as it is resolved.
"""

import json
import os
import sys

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

import reconcile_manifest  # noqa: E402  (path set above)


def baseline():
    with open(os.path.join(ROOT, "k8s", "manifest-drift.json")) as handle:
        return json.load(handle)


@pytest.fixture(scope="module")
def report():
    return reconcile_manifest.drift()


@pytest.mark.parametrize(
    "direction",
    [
        "advertised without a directory",
        "in the cluster but not the gateway",
        "advertised but absent from the cluster",
    ],
)
def test_drift_does_not_grow(report, direction):
    known = set(baseline().get(direction, []))
    assert (
        set(report[direction]) <= known
    ), f"new manifest drift in {direction}: {sorted(set(report[direction]) - known)}"


def test_baseline_only_lists_names_that_still_drift(report):
    stale = {
        direction: sorted(set(baseline().get(direction, [])) - set(names))
        for direction, names in report.items()
        if set(baseline().get(direction, [])) - set(names)
    }
    assert not stale, f"resolved drift still pinned in k8s/manifest-drift.json: {stale}"
