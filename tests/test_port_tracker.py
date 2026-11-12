"""The port tracker, checked against the repository rather than trusted.

`rust/CALCULATOR-PORT.md` is the only place that says what has been ported, so a
wrong entry there is worse than no entry: it is a claim someone will build on.
It has been wrong twice - a substring match once counted fifty-four services on
the Rust engine that were not, and a backlog section once listed forty services
whose directories had been deleted.

So the file is read by this test and its every claim is compared with what is
actually there:

* a service named anywhere in the tracker must exist as a directory, or be in the
  gateway's route table, or have a calculator registered, or be in the pinned
  list of maths with no route;
* the pinned list may shrink but not grow, and a service that gains a route has
  to leave it.
"""

import json
import os
import re

import pytest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TRACKER = os.path.join(ROOT, "rust", "CALCULATOR-PORT.md")
ROUTES = os.path.join(ROOT, "gateway", "go", "config", "services.json")
API = os.path.join(ROOT, "rust", "apps", "api-server", "src", "lib.rs")


# Services the tracker once listed as needing their maths ported, whose
# directories were deleted anyway. Nothing serves them. Each is either a
# duplicate of a capability that survives or an endpoint the platform meant to
# keep; either way it is a decision, not a task, so the set is pinned.
RETIRED_WITHOUT_ENDPOINT = {
    "annual-report-service",
    "bank-fee-analysis-service",
    "capital-budgeting-service",
    "cash-flow-forecasting-service",
    "comparative-financial-statements-service",
    "credit-derivatives-service",
    "credit-risk-analysis-service",
    "discounted-payback-period-service",
    "equivalent-units-service",
    "exotic-derivatives-service",
    "financial-forecasting-service",
    "forward-contracts-service",
    "ifrs-16-lease-accounting-service",
    "income-statement-service",
    "interim-financial-reporting-service",
    "internal-controls-testing-service",
    "labour-cost-variance-service",
    "labour-efficiency-variance-service",
    "labour-rate-variance-service",
    "liquidity-forecast-service",
    "liquidity-management-service",
    "liquidity-risk-service",
    "make-or-buy-decision-service",
    "management-reporting-service",
    "market-risk-service",
    "material-cost-variance-service",
    "material-price-variance-service",
    "material-usage-variance-service",
    "payroll-service",
    "r-and-d-tax-service",
    "regulatory-reporting-service",
    "risk-return-analysis-service",
    "sales-price-variance-service",
    "sales-volume-variance-service",
    "swap-valuation-service",
    "tax-calculation-service",
    "time-value-of-money-service",
    "times-interest-earned-service",
    "treasury-cash-service",
    "treasury-operations-service",
    "variance-service",
    "working-capital-finance-service",
    "working-capital-optimization-service",
    "working-capital-service",
}


def tracker_text():
    with open(TRACKER, encoding="utf-8") as handle:
        return handle.read()


def routed_services():
    with open(ROUTES, encoding="utf-8") as handle:
        document = json.load(handle)
    names = set()

    def walk(node):
        if isinstance(node, dict):
            for key, value in node.items():
                if key in ("name", "service", "target") and isinstance(value, str):
                    names.add(value)
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(document)
    return names


def calculator_names():
    with open(API, encoding="utf-8") as handle:
        source = handle.read()
    return set(re.findall(r'map\.insert\(\s*\n?\s*"([a-z0-9\-]+)"', source))


def tracked_services():
    """Every service named in a `- <name>` bullet, across every section."""
    return re.findall(r"^- ([a-z0-9\-]+-service)\s*(?:->.*)?$", tracker_text(), re.M)


def test_the_tracker_names_services_that_exist_somewhere():
    routed = routed_services()
    on_disk = set(os.listdir(ROOT))
    reachable = routed | on_disk
    calculators = calculator_names()

    stray = []
    for service in tracked_services():
        if service in RETIRED_WITHOUT_ENDPOINT:
            continue
        if service in reachable:
            continue
        stem = service[: -len("-service")]
        if any(name.startswith(stem) for name in calculators):
            continue
        stray.append(service)

    assert not stray, (
        "the tracker names services that are not on disk, not routed, have no calculator and are not "
        f"pinned as unreachable: {sorted(stray)}"
    )


def _calculator_belongs_to(name: str, service: str) -> bool:
    """Whether a calculator name answers a particular service.

    A prefix is not enough. `payroll-accounting-process` starts with `payroll`, but it answers
    payroll-accounting-service; payroll-service is `payroll-run` and `payroll-taxes`. Matching the
    longest service stem that the calculator name begins with is what tells the two apart.
    """
    stems = [other[: -len("-service")] for other in tracked_services() if other.endswith("-service")]
    matching = [stem for stem in stems if name == stem or name.startswith(stem + "-")]
    if not matching:
        return False
    return max(matching, key=len) == service[: -len("-service")]


@pytest.mark.parametrize("service", sorted(RETIRED_WITHOUT_ENDPOINT))
def test_a_retired_capability_has_not_quietly_returned(service):
    """If one of these is routed or registered again, the list and this set both need updating."""
    routed = routed_services()
    reached = service in routed or any(_calculator_belongs_to(name, service) for name in calculator_names())
    assert not reached, f"{service} is reachable again; move it out of RETIRED_WITHOUT_ENDPOINT"


def test_the_wired_section_does_not_list_a_service_twice():
    section = tracker_text().split("## Wired to the Rust library")[1].split("\n## ")[0]
    listed = re.findall(r"^- ([a-z0-9\-]+-service)\s*$", section, re.M)
    duplicates = sorted({name for name in listed if listed.count(name) > 1})
    assert not duplicates, f"listed more than once: {duplicates}"


def test_every_section_that_claims_completion_is_real():
    """A service under 'Wired' must be answered by the Rust engine somehow."""
    section = tracker_text().split("## Wired to the Rust library")[1].split("\n## ")[0]
    wired = re.findall(r"^- ([a-z0-9\-]+-service)\s*$", section, re.M)
    routed = routed_services()
    calculators = calculator_names()
    orphans = [
        service
        for service in wired
        if service not in routed and not any(name.startswith(service[: -len("-service")]) for name in calculators)
    ]
    assert not orphans, (
        "these are claimed as wired to the Rust library but neither routed nor registered as a "
        f"calculator: {sorted(orphans)}"
    )
