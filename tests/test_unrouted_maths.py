"""Maths that exists and nothing can reach.

Four duplicates were hidden by the same illusion: a module that is `pub`, re-exported, and named
nowhere that runs. `scripts/module_reachability.py` finds those by following imports from the only
three things that can call into the library - the API server, the native extension, and the bridge.

This test uses it to hold the port tracker honest. The tracker claims each service below is written
and unreachable; if someone routes the module, the claim stops being true and this test says so
instead of leaving a stale line in a document.
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts"))

import module_reachability  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# service -> the module holding its maths, which the tracker says no door reaches.
# A name here means the tracker still calls that maths unreachable, so the
# reachability script must agree - the test fails if the module gains a door.
#
# These three were reached only by the PyO3 extension, which is gone. They are not duplicates of what
# the engine serves - progressive brackets and a tax provision are not tax::statutory - but nothing can
# call them now, so they are a decision rather than a silent gap: give each a route, or delete it.
UNROUTED = {
    "corporate-tax-service": "tax::corporate",
    "labour-variance-service": "cost_accounting::labour_variance",
    "comparative-statements-service": "ratios_statements::comparative",
}


def _unreachable():
    cwd = os.getcwd()
    os.chdir(ROOT)
    try:
        return set(module_reachability.unreachable_modules())
    finally:
        os.chdir(cwd)


def test_a_crate_root_reexport_is_read_rather_than_guessed():
    """The gate used to guess which module defines a re-exported function.

    `assets::amortization_from_json` is a door's way of naming `assets::amortization`: the function is defined in
    `amortization.rs` and re-exported by lib.rs. The checker used to resolve that by asking "which module defines
    `amortization_from_json`" and taking the first answer the file system offered, which read `budgeting::cash` as
    reachable on one run and dead on the next. It now reads the `pub use` line, which is what the compiler reads.
    """
    reexports = module_reachability._reexports()
    assert reexports[("vimbai_assets", "amortization_from_json")] == ("amortization",)
    assert reexports[("vimbai_assets", "amortization")] == ("amortization",)


def test_the_verdict_does_not_depend_on_iteration_order():
    """Two runs in one process must agree, and the answer must be complete rather than first-found."""
    first = module_reachability.unreachable_modules()
    second = module_reachability.unreachable_modules()
    assert first == second
    # A module the re-export reading resolves must not also be reported dead: that was the bug's signature.
    assert "assets::amortization" not in first
    assert "service_accounting::ifrs_reports" not in first


def test_the_maths_the_tracker_calls_unreachable_is_still_unreachable():
    dead = _unreachable()
    routed = sorted(f"{service} -> {module}" for service, module in UNROUTED.items() if module not in dead)
    assert not routed, (
        "these modules are now reachable, so the port tracker no longer has a gap to declare for "
        f"them - move the service out of the unrouted list: {routed}"
    )


def test_the_checker_still_recognises_wired_maths():
    """If the checker broke, every module would look dead and the test above would pass for free."""
    dead = _unreachable()
    for module in (
        "tax::statutory",
        "treasury_fx::derivatives",
        "cost_accounting::standard_cost",
        "equity_payroll::payroll_accounting",
        "ratios_statements::variance",
        "service_treasury::cash_optimization",
        "capital_valuation::valuation",
        "ratios_statements::statements",
        "ifrs::revenue",
        "ifrs::share_based",
        "ifrs::instruments",
        "capital_valuation::corporate_events",
        "capital_valuation::merger",
    ):
        assert module not in dead, f"{module} is routed, so the checker is not reading it"
