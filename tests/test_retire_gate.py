"""The retirement gate must be followed, not just reported.

Nothing ran scripts/retire_gate.py, so a service could become provably safe to retire and
sit in the gateway indefinitely. This pins the verdicts: when the gate says a routed
service passes every check, this test fails until somebody retires it, and the services
kept in Python are named here so a new one cannot join them quietly.
"""

import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent

# Kept in Python on purpose, and the reason is written down rather than assumed:
# absorption-costing-service is the oracle tests/test_cost_accounting_seam.py compares the engine against - the only
# proof the port reproduces the service it replaced. The record holders this set used to name (job-costing-service)
# have been retired along with the rest of the costing domain.
RECORD_HOLDERS = {
    "absorption-costing-service",
}


@pytest.fixture(scope="module")
def gate():
    result = subprocess.run(
        [sys.executable, "scripts/retire_gate.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    assert result.stdout, result.stderr
    return result.stdout


def summary(gate, marker):
    for line in gate.splitlines():
        if marker in line:
            return line
    raise AssertionError(f"the gate printed no line mentioning {marker}")


def test_no_service_is_left_waiting_to_be_retired(gate):
    line = summary(gate, "routed and safe to retire now")
    assert line.endswith("none"), line


def test_only_record_holders_stay_in_python(gate):
    line = summary(gate, "keep:")
    kept = {name.strip() for name in line.split("keep:", 1)[1].split(",") if name.strip()}
    assert kept == RECORD_HOLDERS, kept
