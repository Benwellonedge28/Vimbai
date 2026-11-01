"""
Deduplication regression tests.

Two clone families were de-registered as duplicated services (source
directories stay in the repository; only registration was removed):

1. Risk family - investigation-service, risk-mitigation-service and
   risk-reporting-service were exact clones of risk-assessment-service
   (same models, same :RiskItem label + :OWNS_RISK edge, same endpoints).

2. Platform-template family - ai-readiness, cqrs-event-sourcing,
   data-sovereignty, disaster-recovery, edge-computing, event-streaming,
   federated-network, grpc, high-availability, infrastructure-as-code,
   multi-cloud, multi-tenant, observability, plugin-extension and
   offline-sync services were all the same generic Entity CRUD template
   ("platform capability" placeholders with zero domain logic, none
   referenced by the frontend).

3. Calculator family - treasury-operations-service and treasury-cash-service
   were stateless calculators (no session, no persistence, no frontend
   reference) whose figures now come from the Rust library. Their source
   stays as the reference implementation; only registration was removed.

4. Cost-calculator family - the nineteen stateless cost-accounting
   calculators now living in the Rust vimbai_cost_accounting crate. They
   leave the gateway and the cluster (no caller, no state) but KEEP their
   CI coverage: the Python maths is the reference the parity tests check
   the Rust against, so its regression tests stay in test-core.yml.

5. Variance and budget calculators - nine variance services (the
   vimbai_variance crate) and six budget services (the vimbai_budgeting
   crate). Same treatment as the cost calculators: out of the gateway and
   the cluster, retained in CI as the parity reference.

6. Absorption and margin calculators - over/under absorption and margin
   of safety waited until the cost-accounting crate could reproduce them;
   once it did they joined the same de-registered, CI-retained treatment.

7. Scenario budgeting - ported into the budgeting crate, so it joins the
   de-registered, CI-retained treatment. Rolling forecast stays routed on
   purpose: it stores forecasts, so it is a record keeper rather than a
   calculator.

These tests pin every family to its de-registered state.
"""

import json
import os
import re

import pytest

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

RISK_DUPLICATES = {
    "investigation-service",
    "risk-mitigation-service",
    "risk-reporting-service",
}

PLATFORM_TEMPLATE_DUPLICATES = {
    "ai-readiness-service",
    "cqrs-event-sourcing-service",
    "data-sovereignty-service",
    "disaster-recovery-service",
    "edge-computing-service",
    "event-streaming-service",
    "federated-network-service",
    "grpc-service",
    "high-availability-service",
    "infrastructure-as-code-service",
    "multi-cloud-service",
    "multi-tenant-service",
    "observability-service",
    "plugin-extension-service",
    "offline-sync-service",
}

CALCULATOR_PORTS = {
    "treasury-operations-service",
    "treasury-cash-service",
}

COST_CALCULATOR_PORTS = {
    "activity-based-costing-service",
    "actual-cost-service",
    "budgeted-cost-service",
    "standard-cost-service",
    "target-costing-service",
    "throughput-accounting-service",
    "make-or-buy-decision-service",
    "order-acceptance-service",
    "limiting-factor-service",
    "prime-cost-service",
    "lifecycle-costing-service",
    "cost-accounting-service",
    "cost-centre-service",
    "fixed-cost-service",
    "variable-cost-service",
    "overhead-apportionment-service",
    "marginal-costing-service",
    "total-production-cost-service",
    "equivalent-units-service",
}

VARIANCE_BUDGET_PORTS = {
    "variance-service",
    "material-cost-variance-service",
    "material-price-variance-service",
    "material-usage-variance-service",
    "labour-cost-variance-service",
    "labour-rate-variance-service",
    "labour-efficiency-variance-service",
    "sales-price-variance-service",
    "sales-volume-variance-service",
    "master-budget-service",
    "cash-budget-service",
    "capital-expenditure-budget-service",
    "flexible-budget-service",
    "zero-based-budget-service",
    "budget-service",
}

SCENARIO_BUDGET_PORTS = {"scenario-budget-service"}

# IFRS 2, 9, 15 and 16 are the cleanest transpositions in the library: pure
# calculators with no state, so the Rust crates own the answers now.
IFRS_CALCULATOR_PORTS = {
    "ifrs-2-share-based-payment-service",
    "ifrs-9-financial-instruments-service",
    "ifrs-15-revenue-recognition-service",
    "ifrs-16-lease-accounting-service",
}

# Statements and the payroll pair: same story, straight transpositions of
# statements.rs, income_statement.rs, payroll.rs and payroll_accounting.rs.
STATEMENT_PAYROLL_PORTS = {
    "financial-statements-service",
    "income-statement-service",
    "payroll-service",
    "payroll-accounting-service",
}

# The tax pair retires only after the seam reproduces their figures; those
# parity cases live in tests/test_native_seam.py.
# Investor return metrics, ported into ratios_statements::investor_metrics and
# proven field-for-field by tests/test_native_seam.py.
INVESTOR_METRIC_PORTS = {
    "du-pont-service",
    "asset-turnover-service",
    "croic-service",
    "eva-service",
    "eps-service",
}

# fixed-assets-service retires on the same evidence, in the assets family.
FIXED_ASSETS_PORTS = {
    "fixed-assets-service",
    "depreciation-service",
    "amortization-service",
}

TAX_PORTS = {
    "corporate-tax-service",
    "tax-calculation-service",
}

ABSORPTION_MARGIN_PORTS = {
    "over-under-absorption-service",
    "margin-safety-service",
}

ALL_DEREGISTERED = (
    RISK_DUPLICATES | PLATFORM_TEMPLATE_DUPLICATES | CALCULATOR_PORTS | COST_CALCULATOR_PORTS | VARIANCE_BUDGET_PORTS
)

# Families whose Python code is no longer run anywhere, so their names must stay
# out of the CI matrix too. The cost calculators keep their tests by design.
UNTESTED_AFTER_DEREGISTRATION = RISK_DUPLICATES | PLATFORM_TEMPLATE_DUPLICATES | CALCULATOR_PORTS


def test_gateway_has_no_duplicate_routes():
    routes = json.load(open(os.path.join(_ROOT, "api-gateway", "config", "services.json")))["services"]
    registered = {r["name"] for r in routes}
    for dup in ALL_DEREGISTERED:
        assert dup not in registered, f"{dup} must stay de-registered from the gateway"
    # the canonical services stay routed
    assert "risk-assessment-service" in registered
    # treasury analytics keeps answering, on the Rust engine
    assert "treasury-analytics-service" in registered


@pytest.mark.parametrize("dup", sorted(ALL_DEREGISTERED))
def test_duplicate_not_deployed(dup):
    for manifest in ("k8s/all-services.yaml", "k8s/new-services.yaml", "k8s/new-services-v2.yaml"):
        path = os.path.join(_ROOT, manifest)
        if not os.path.isfile(path):
            continue
        content = open(path).read()
        assert not re.search(
            r"^  name: %s$" % re.escape(dup), content, re.M
        ), f"{dup} must not be deployed via {manifest}"


@pytest.mark.parametrize("dup", sorted(UNTESTED_AFTER_DEREGISTRATION))
def test_duplicate_not_in_ci_brackets(dup):
    ci = open(os.path.join(_ROOT, ".github", "workflows", "test-core.yml")).read()
    assert not re.search(r"\b" + re.escape(dup) + r"\b", ci), f"{dup} must stay out of CI bracket lists"


@pytest.mark.parametrize("dup", sorted(ALL_DEREGISTERED))
def test_duplicate_source_dir_still_exists(dup):
    # standing rule: no repository files are ever deleted
    assert os.path.isdir(os.path.join(_ROOT, dup))
    assert os.path.isfile(os.path.join(_ROOT, dup, "main.py"))


def test_brackets_do_not_mount_ported_calculators():
    for bracket in ("treasury-banking-bracket",):
        members = json.load(open(os.path.join(_ROOT, "brackets", bracket, "members.json")))
        names = {m["name"] for m in members} if isinstance(members, list) else set(members)
        for ported in CALCULATOR_PORTS:
            assert ported not in names, f"{bracket} must not mount {ported}"


@pytest.mark.parametrize("ported", sorted(CALCULATOR_PORTS))
def test_ported_calculator_keeps_its_reference_implementation(ported):
    # the Python maths stays as the reference the Rust port is tested against
    assert os.path.isfile(os.path.join(_ROOT, ported, "main.py"))


@pytest.mark.parametrize(
    "ported", sorted(COST_CALCULATOR_PORTS | VARIANCE_BUDGET_PORTS | ABSORPTION_MARGIN_PORTS | SCENARIO_BUDGET_PORTS)
)
def test_cost_calculator_keeps_its_ci_coverage(ported):
    # the Python reference maths stays tested even though it is not deployed
    ci = open(os.path.join(_ROOT, ".github", "workflows", "test-core.yml")).read()
    assert ported in ci, f"{ported} reference tests must stay wired into CI"


def test_cost_calculators_absent_from_brackets():
    for bracket in ("costing-budgeting-bracket",):
        members = json.load(open(os.path.join(_ROOT, "brackets", bracket, "members.json")))
        names = {m["name"] for m in members} if isinstance(members, list) else set(members)
        overlap = names & (
            COST_CALCULATOR_PORTS | VARIANCE_BUDGET_PORTS | ABSORPTION_MARGIN_PORTS | SCENARIO_BUDGET_PORTS
        )
        assert not overlap, f"{bracket} still mounts {sorted(overlap)}"
