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
UNROUTED = {
    "business-valuation-service": "capital_valuation::valuation",
    "deal-structuring-service": "capital_valuation::corporate_events",
    "financial-reporting-service": "ratios_statements::statements",
    "financial-statements-service": "ratios_statements::statements",
    "ifrs-15-revenue-recognition-service": "ifrs::revenue",
    "ifrs-2-share-based-payment-service": "ifrs::share_based",
    "ifrs-9-financial-instruments-service": "ifrs::instruments",
    "kpi-tracking-service": "reporting::analysis",
    "merger-valuation-service": "capital_valuation::merger",
    "post-merger-service": "capital_valuation::corporate_events",
    "profit-loss-account-service": "ratios_statements::profit_loss",
    "scenario-budget-service": "budgeting::scenario",
    "suspense-error-service": "ratios_statements::ledger_controls",
    "synergy-analysis-service": "capital_valuation::corporate_events",
}


def _unreachable():
    cwd = os.getcwd()
    os.chdir(ROOT)
    try:
        return set(module_reachability.unreachable_modules())
    finally:
        os.chdir(cwd)


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
        "tax::corporate",
        "treasury_fx::derivatives",
        "cost_accounting::labour_variance",
        "equity_payroll::payroll_accounting",
        "ratios_statements::comparative",
        "service_treasury::cash_optimization",
    ):
        assert module not in dead, f"{module} is routed, so the checker is not reading it"
