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

import glob
import json
import os
import pathlib
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
    "capital-expenditure-budget-service",
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
    "profit-loss-account-service",
    "payroll-service",
    "payroll-accounting-service",
}

# The tax pair retires only after the seam reproduces their figures; those
# parity cases live in tests/test_native_seam.py.
# The hedging and FX family, ported into treasury_fx and proven body-for-body
# by tests/test_native_seam.py.
HEDGING_FX_PORTS = {
    "capital-budgeting-service",
    "financial-forecasting-service",
    "labour-cost-variance-service",
    "material-cost-variance-service",
    "regulatory-reporting-service",
    "management-reporting-service",
    "financial-reporting-service",
    "annual-report-service",
    "interim-financial-reporting-service",
    "comparative-financial-statements-service",
    "segment-reporting-service",
    "working-capital-service",
    "working-capital-optimization-service",
    "working-capital-finance-service",
    "cash-flow-forecasting-service",
    "liquidity-forecast-service",
    "liquidity-management-service",
    "liquidity-risk-service",
    "credit-derivatives-service",
    "exotic-derivatives-service",
    "forward-contracts-service",
    "multi-currency-service",
    "currency-hedging-service",
    "cross-currency-swap-service",
    "swap-valuation-service",
    "futures-hedging-service",
    "commodity-hedging-service",
    "interest-rate-hedging-service",
    "foreign-currency-translation-service",
}

# Capital-return calculators, ported into capital_valuation::capital_returns.
CAPITAL_RETURN_PORTS = {
    "cost-of-capital-service",
    "discount-factor-service",
    "discounted-payback-period-service",
    "accounting-rate-return-service",
    # second batch: time_value.rs, growth.rs, allocation.rs, appraisal.rs
    "time-value-of-money-service",
    "present-value-service",
    "mva-service",
    "sustainable-growth-service",
}

# Risk-adjusted returns, business valuation and merger valuation, ported into
# capital_valuation::{risk_return, valuation, merger}.
VALUATION_PORTS = {
    "risk-return-analysis-service",
    "business-valuation-service",
    "merger-valuation-service",
}

# Corporate events, ported into capital_valuation::corporate_events.
CORPORATE_EVENT_PORTS = {
    "deal-structuring-service",
    "divestiture-service",
    "post-merger-service",
    "synergy-analysis-service",
}

# Investor return metrics, ported into ratios_statements::investor_metrics and
# proven field-for-field by tests/test_native_seam.py.
INVESTOR_METRIC_PORTS = {
    "du-pont-service",
    "asset-turnover-service",
    "croic-service",
    "eva-service",
    "eps-service",
}

# Module-level containers that LOOK like stores but are not: seed catalogues
# that nothing ever writes, and auth material that is ephemeral by design.
SEED_CATALOGUES = {
    # static template definitions; /templates only ever reads them
    "notifications-service": {"templates"},
    # short-lived session/challenge/token state, deliberately not persisted
    "identity-service": {"sessions", "mfa_codes", "password_reset_tokens", "refresh_tokens"},
    "mfa-auth-service": {"_pending_challenges"},
    "cache-service": {"_cache", "_cache_stats"},
    # money movement lives outside Vimbai's remit; the gateway keeps its own records
    "payment-gateway-service": {"customers", "payment_methods", "payments", "refunds", "disputes"},
}

# The four audit families share one service now: audit-engagement-service,
# with audit_type telling them apart. The others were merged away, not ported.
AUDIT_ENGAGEMENT_MERGE = {
    "tax-audit-service",
    "forensic-accounting-service",
    "it-audit-service",
    "operational-audit-service",
}

# Advisory endpoints, ported into the vimbai_advisory crate. Record-free, so
# they were deleted once the port and its parity cases landed.
ADVISORY_PORTS = {
    "cash-pooling-service",
    "construction-contracts-service",
    "debt-covenants-service",
    "director-emoluments-service",
    "profitability-index-service",
    "share-options-service",
    "related-party-service",
    "events-after-reporting-service",
    "household-finance-service",
    "fund-management-service",
    "interest-rate-risk-service",
    "investment-portfolio-service",
    "transfer-pricing-service",
    "analytical-procedures-service",
}

# Ledger study endpoints, ported into vimbai_ledger_core::analytics. None held a
# record; each was one /analyze over numbers the caller already had.
LEDGER_ANALYTICS_PORTS = {
    "accounts-payable-service",
    "accounts-receivable-service",
    "general-ledger-service",
    "journal-entries-service",
    "analytics-service",
}

# Reporting diagnostics, ported into reporting::analysis.
REPORT_ANALYSIS_PORTS = {
    "bank-fee-analysis-service",
    "kpi-tracking-service",
    "suspense-error-service",
    "times-interest-earned-service",
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

# The owner asked for the Python source of these curved-away calculators to be deleted once
# the Rust implementation was proven, rather than kept as a parity oracle. Their maths is
# covered by the Rust crates' own tests, with the figures captured from the services before
# deletion, so their absence is the expected state and the check below enforces it.
# Services de-registered from the gateway because every published path is answered by a Rust
# route. Kept on disk as the parity oracle for the port.
RETIRED_PORTS = set()

DELETED_PORTS = {
    "ai-readiness-service",
    "bank-fee-analysis-service",
    "business-valuation-service",
    "cash-flow-forecasting-service",
    "comparative-financial-statements-service",
    "cqrs-event-sourcing-service",
    "credit-derivatives-service",
    "data-sovereignty-service",
    "deal-structuring-service",
    "disaster-recovery-service",
    "discounted-payback-period-service",
    "edge-computing-service",
    "equivalent-units-service",
    "event-streaming-service",
    "exotic-derivatives-service",
    "federated-network-service",
    "financial-forecasting-service",
    "financial-reporting-service",
    "financial-statements-service",
    "fixed-cost-service",
    "forward-contracts-service",
    "fraud-detection-service",
    "grpc-service",
    "high-availability-service",
    "ifrs-15-revenue-recognition-service",
    "ifrs-16-lease-accounting-service",
    "ifrs-2-share-based-payment-service",
    "ifrs-9-financial-instruments-service",
    "income-statement-service",
    "infrastructure-as-code-service",
    "interim-financial-reporting-service",
    "investigation-service",
    "kpi-tracking-service",
    "labour-cost-variance-service",
    "labour-efficiency-variance-service",
    "labour-rate-variance-service",
    "lifecycle-costing-service",
    "liquidity-management-service",
    "make-or-buy-decision-service",
    "management-reporting-service",
    "material-cost-variance-service",
    "material-price-variance-service",
    "material-usage-variance-service",
    "merger-valuation-service",
    "multi-cloud-service",
    "multi-tenant-service",
    "observability-service",
    "offline-sync-service",
    "over-under-absorption-service",
    "payroll-accounting-service",
    "payroll-service",
    "plugin-extension-service",
    "post-merger-service",
    "profit-loss-account-service",
    "profit-service",
    "regulatory-reporting-service",
    "risk-mitigation-service",
    "risk-reporting-service",
    "risk-return-analysis-service",
    "sales-price-variance-service",
    "sales-volume-variance-service",
    "scenario-budget-service",
    "supply-chain-service",
    "suspense-error-service",
    "swap-valuation-service",
    "synergy-analysis-service",
    "tax-calculation-service",
    "time-value-of-money-service",
    "times-interest-earned-service",
    "treasury-cash-service",
    "treasury-operations-service",
    "variable-cost-service",
    "variance-service",
    "working-capital-finance-service",
    "working-capital-optimization-service",
    "working-capital-service",
    "trial-balance-service",
    "flexible-budget-service",
    "master-budget-service",
    "zero-based-budget-service",
    "cash-budget-service",
    "investment-appraisal-service",
    "capital-allocation-service",
    "portfolio-optimization-service",
    "project-appraisal-service",
    "asset-allocation-service",
    "accounting-rate-return-service",
    # retired through the calculation API, verified by the parity harness
    "mva-service",
    "sustainable-growth-service",
    "commodity-hedging-service",
    "cross-currency-swap-service",
    "currency-hedging-service",
    "discount-factor-service",
    "foreign-currency-translation-service",
    "futures-hedging-service",
    "interest-rate-hedging-service",
    "multi-currency-service",
    "segment-reporting-service",
    "annual-report-service",
    "capital-budgeting-service",
    "cost-centre-service",
    "credit-risk-analysis-service",
    "depreciation-service",
    "internal-controls-testing-service",
    "limiting-factor-service",
    "liquidity-forecast-service",
    "liquidity-risk-service",
    "margin-safety-service",
    "marginal-costing-service",
    "market-risk-service",
    "order-acceptance-service",
    "overhead-apportionment-service",
    "prime-cost-service",
    "r-and-d-tax-service",
    "total-production-cost-service",
    "activity-based-costing-service",
    "actual-cost-service",
    "amortization-service",
    "asset-turnover-service",
    "balanced-scorecard-service",
    "budgeted-cost-service",
    "capital-expenditure-budget-service",
    "consolidation-service",
    "corporate-tax-service",
    "cost-accounting-service",
    "cost-of-capital-service",
    "croic-service",
    "divestiture-service",
    "du-pont-service",
    "eps-service",
    "eva-service",
    "financial-distress-prediction-service",
    "fixed-assets-service",
    "fraud-risk-assessment-service",
    "goodwill-service",
    "government-grants-service",
    "group-tax-service",
    "internal-control-evaluation-service",
    "inventory-valuation-service",
    "net-realizable-value-service",
    "options-pricing-service",
    "partnership-accounting-service",
    "partnership-sale-service",
    "payment-optimization-service",
    "pension-accounting-service",
    "regulatory-ratio-service",
    "standard-cost-service",
    "substantive-testing-service",
    "target-costing-service",
    "tax-provision-service",
    "tax-risk-service",
    "throughput-accounting-service",
    "vat-reporting-service",
}

ALL_DEREGISTERED = (
    RISK_DUPLICATES
    | PLATFORM_TEMPLATE_DUPLICATES
    | CALCULATOR_PORTS
    | COST_CALCULATOR_PORTS
    | VARIANCE_BUDGET_PORTS
    | CAPITAL_RETURN_PORTS
    | VALUATION_PORTS
    | CORPORATE_EVENT_PORTS
    | REPORT_ANALYSIS_PORTS
    | LEDGER_ANALYTICS_PORTS
    | ADVISORY_PORTS
    | AUDIT_ENGAGEMENT_MERGE
    | RETIRED_PORTS
)

# Families whose Python code is no longer run anywhere, so their names must stay
# out of the CI matrix too. The cost calculators keep their tests by design.
UNTESTED_AFTER_DEREGISTRATION = RISK_DUPLICATES | PLATFORM_TEMPLATE_DUPLICATES | CALCULATOR_PORTS


def test_gateway_has_no_duplicate_routes():
    routes = json.load(open(os.path.join(_ROOT, "gateway", "go", "config", "services.json")))["services"]
    registered = {r["name"] for r in routes}
    for dup in ALL_DEREGISTERED:
        assert dup not in registered, f"{dup} must stay de-registered from the gateway"
    # the canonical services stay routed
    assert "risk-assessment-service" in registered
    # treasury analytics keeps answering, on the Rust engine
    assert "treasury-analytics-service" in registered


@pytest.mark.parametrize("dup", sorted(ALL_DEREGISTERED))
def test_duplicate_not_deployed(dup):
    for manifest in (
        "infrastructure/kubernetes/all-services.yaml",
        "infrastructure/kubernetes/new-services.yaml",
        "infrastructure/kubernetes/new-services-v2.yaml",
    ):
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
def test_duplicate_source_is_accounted_for(dup):
    # Ported duplicates keep their Python source as the parity oracle. The one
    # family the owner asked to have removed outright (the ledger study
    # endpoints) has no directory, and is proven by the Rust crate instead.
    source = os.path.join(_ROOT, dup)
    if dup in LEDGER_ANALYTICS_PORTS or dup in ADVISORY_PORTS or dup in AUDIT_ENGAGEMENT_MERGE:
        assert not os.path.exists(source), f"{dup} was ported and deleted"
        if dup in AUDIT_ENGAGEMENT_MERGE:
            assert "audit-engagement-service" in open(os.path.join(_ROOT, "docs", "SERVICE-CONSOLIDATION.md")).read()
            return
        marker = "ledger_core::analytics" if dup in LEDGER_ANALYTICS_PORTS else "vimbai_advisory"
        assert marker in open(os.path.join(_ROOT, "docs", "SERVICE-CONSOLIDATION.md")).read()
        return
    if dup in DELETED_PORTS:
        assert not os.path.exists(source), f"{dup} has a Rust replacement and must be gone"
        return
    assert os.path.isdir(source)
    assert os.path.isfile(os.path.join(source, "main.py"))


def test_brackets_do_not_mount_ported_calculators():
    for bracket in ("treasury-banking-bracket",):
        members = json.load(open(os.path.join(_ROOT, "brackets", bracket, "members.json")))
        names = {m["name"] for m in members} if isinstance(members, list) else set(members)
        for ported in CALCULATOR_PORTS:
            assert ported not in names, f"{bracket} must not mount {ported}"


@pytest.mark.parametrize("ported", sorted(CALCULATOR_PORTS))
def test_ported_calculator_keeps_its_evidence(ported):
    # 2026-11-05: Samuel asked for the retired Python sources to be deleted outright, so the
    # reference implementation is no longer the evidence. The recorded request/response captures
    # in rust/api/tests/parity are, and they are what the Rust port is tested against.
    assert not os.path.exists(os.path.join(_ROOT, ported)), f"{ported} was deleted on instruction"
    captures = os.path.join(_ROOT, "rust", "crates", "api", "tests", "parity")
    stem = ported[: -len("-service")]
    has_capture = any(f.startswith(stem) for f in os.listdir(captures))
    port_map = open(os.path.join(_ROOT, "rust", "CALCULATOR-PORT.md")).read()
    assert (
        has_capture or ported in port_map
    ), f"{ported} needs a parity capture or a line in CALCULATOR-PORT.md to stand in for its deleted source"


@pytest.mark.parametrize(
    "ported", sorted(COST_CALCULATOR_PORTS | VARIANCE_BUDGET_PORTS | ABSORPTION_MARGIN_PORTS | SCENARIO_BUDGET_PORTS)
)
def test_cost_calculator_keeps_its_ci_coverage(ported):
    # A ported calculator whose Python source is gone must not appear in any CI list; one
    # that still keeps its source stays tested even though it is not deployed.
    ci = open(os.path.join(_ROOT, ".github", "workflows", "test-core.yml")).read()
    if ported in DELETED_PORTS:
        assert ported not in ci, f"{ported} has no source left to test"
        return
    assert ported in ci, f"{ported} reference tests must stay wired into CI"


def test_cost_calculators_absent_from_brackets():
    for bracket in ("costing-budgeting-bracket",):
        members = json.load(open(os.path.join(_ROOT, "brackets", bracket, "members.json")))
        names = {m["name"] for m in members} if isinstance(members, list) else set(members)
        overlap = names & (
            COST_CALCULATOR_PORTS | VARIANCE_BUDGET_PORTS | ABSORPTION_MARGIN_PORTS | SCENARIO_BUDGET_PORTS
        )
        assert not overlap, f"{bracket} still mounts {sorted(overlap)}"


def test_only_triaged_containers_remain():
    """No untriaged module-level store may appear without a decision here."""
    import ast

    found = {}
    for path in glob.glob(os.path.join(_ROOT, "*/main.py")):
        svc = os.path.basename(os.path.dirname(path))
        if svc == "brackets":
            continue
        with open(path) as handle:
            tree = ast.parse(handle.read())
        names = set()
        for node in tree.body:
            if isinstance(node, ast.Assign) and isinstance(node.value, (ast.Dict, ast.List)):
                names.update(t.id for t in node.targets if isinstance(t, ast.Name) and not t.id.isupper())
            if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and not node.target.id.isupper():
                names.add(node.target.id)
        if names:
            found[svc] = names

    for svc, names in found.items():
        allowed = SEED_CATALOGUES.get(svc, set())
        assert names <= allowed, f"{svc} holds untriaged state: {sorted(names - allowed)}"


def _gateway_routes():
    routes = json.load(open(os.path.join(_ROOT, "gateway", "go", "config", "services.json")))["services"]
    return {r["name"] for r in routes}


def test_profit_service_paths_are_answered_by_rust():
    """De-registered, so every path it used to answer has to exist in the registry."""
    assert "profit-service" not in _gateway_routes()
    registry = open(os.path.join(_ROOT, "rust", "crates", "api", "src", "lib.rs")).read()
    for path in (
        "calculate",
        "from-cash-flow",
        "average-profit",
        "average-accounting-profit",
        "net-profit",
        "profit-compare",
    ):
        assert f'"profit-{path}"' in registry, f"profit-{path} is not served"


def test_retired_ports_keep_their_files():
    for service in RETIRED_PORTS:
        assert os.path.isfile(os.path.join(_ROOT, service, "main.py")), f"{service} was deleted"
