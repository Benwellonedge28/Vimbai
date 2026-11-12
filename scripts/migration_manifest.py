"""Writes docs/MIGRATION-MANIFEST.md: every routed service and the domain that will own it.

The manifest is derived, not typed by hand. gateway/go/config/services.json is the only list of
what the product actually serves, so it is the one that gets a row per service; the domain each row
is assigned to comes from OWNERS below. A routed service with no owner is reported as an error
rather than silently left out, because an unowned service is exactly what grows into a duplicate
later.

Run after adding, retiring or reassigning a service; tests/test_migration_manifest.py fails if the
document no longer matches the generator.
"""

import json
import os
import re

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVICES = os.path.join(REPO, "gateway", "go", "config", "services.json")
PORT_MAP = os.path.join(REPO, "rust", "CALCULATOR-PORT.md")
OUT = os.path.join(REPO, "docs", "MIGRATION-MANIFEST.md")

# Domain -> what it is responsible for, and where its code lands.
DOMAINS = {
    "kernel": (
        "ledger-core: sole authority for posting integrity, immutability and the hash chain",
        "rust/crates/kernel/ledger-core",
    ),
    "accounting": ("accounting workflows and the statements they produce", "rust/crates/domains/accounting"),
    "receivables": ("sales ledger, bad debts and provisions against them", "rust/crates/domains/receivables"),
    "payables": ("purchase ledger and expense capture", "rust/crates/domains/payables"),
    "invoicing": ("issuing and tracking invoices", "rust/crates/domains/invoicing"),
    "banking": ("bank connectivity and reconciliation", "rust/crates/domains/banking"),
    "payments": ("Vimbai Pay: orchestration, lifecycle and providers", "rust/crates/domains/payments"),
    "budgeting": ("budgets, forecasts and scenario planning", "rust/crates/domains/budgeting"),
    "costing": ("cost accumulation, apportionment, product and job costing", "rust/crates/domains/costing"),
    "treasury": ("cash, liquidity, debt and currency management", "rust/crates/domains/treasury"),
    "taxation": ("tax accounting, planning and compliance", "rust/crates/domains/taxation"),
    "assets": ("fixed, intangible, biological and investment assets", "rust/crates/domains/assets"),
    "equity": ("share capital, reserves and equity movements", "rust/crates/domains/equity"),
    "partnerships": ("partnerships, groups and non-profit entities", "rust/crates/domains/partnerships"),
    "payroll": ("payroll and benefits", "rust/crates/domains/payroll"),
    "reporting": ("report assembly and distribution", "rust/crates/domains/reporting"),
    "analytics": ("benchmarking, warehousing and data movement", "rust/crates/domains/analytics"),
    "finance": ("financial management and decision support", "rust/crates/domains/finance"),
    "audit": ("audit engagements, sampling and evidence", "rust/crates/domains/audit"),
    "compliance": ("regulatory and controls compliance", "rust/crates/domains/compliance"),
    "risk": ("risk assessment and mitigation", "rust/crates/domains/risk"),
    "workflow": ("workflow definitions and long-running orchestration", "rust/crates/domains/workflow"),
    "automation": ("scheduled and triggered automation", "rust/crates/domains/automation"),
    "multimodal": ("document, image and audio understanding", "rust/crates/domains/multimodal"),
    "documents": ("document storage and rendering", "rust/crates/platform/documents"),
    "integrations": ("webhooks, GraphQL and external feeds", "rust/crates/platform/integrations"),
    "calculations": ("the calculation surface the client calls", "rust/crates/application"),
    "platform": ("identity, security, events, caching, sync and observability", "rust/crates/platform"),
}

# Every routed service, and the domain that owns it.
OWNERS = {
    "double-entry-principles-service": "kernel",
    "financial-integrity-service": "kernel",
    "financial-state-machine-service": "kernel",
    "control-account-reconciliation-service": "kernel",
    "accounting-service": "accounting",
    "accounting-standards-service": "accounting",
    "company-accounting-service": "accounting",
    "departmental-accounting-service": "accounting",
    "balance-sheet-service": "accounting",
    "cash-flow-service": "accounting",
    "cash-flow-statement-service": "accounting",
    "cashbook-service": "accounting",
    "fund-accounting-service": "accounting",
    "trading-account-service": "accounting",
    "intercompany-service": "accounting",
    "ifrs-reporting-service": "accounting",
    "revenue-recognition-service": "accounting",
    "bad-debts-recovery-service": "receivables",
    "provision-bad-debts-service": "receivables",
    "provision-doubtful-debts-service": "receivables",
    "sales-ledger-control-service": "receivables",
    "expense-tracking-service": "payables",
    "purchases-ledger-control-service": "payables",
    "invoicing-service": "invoicing",
    "bank-feed-service": "banking",
    "bank-reconciliation-service": "banking",
    "bank-relationship-service": "banking",
    "banking-integration-service": "banking",
    "currency-service": "banking",
    "payment-gateway-service": "payments",
    "pos-integration-service": "payments",
    "subscription-plans-service": "payments",
    "activity-based-budget-service": "budgeting",
    "appropriation-control-service": "budgeting",
    "rolling-forecast-service": "budgeting",
    "scenario-analysis-service": "budgeting",
    "scenario-modeling-service": "budgeting",
    "sensitivity-analysis-service": "budgeting",
    "zero-based-budgeting-service": "budgeting",
    "absorption-costing-service": "costing",
    "absorption-costing-statement-service": "costing",
    "basis-apportionment-service": "costing",
    "cvp-analysis-service": "costing",
    "job-costing-service": "costing",
    "overhead-absorption-rate-service": "costing",
    "process-costing-service": "costing",
    "product-costing-service": "costing",
    "selling-price-reduction-service": "costing",
    "cash-management-service": "treasury",
    "cash-optimization-service": "treasury",
    "debt-management-service": "treasury",
    "foreign-exchange-service": "treasury",
    "petty-cash-service": "treasury",
    "sovereign-treasury-service": "treasury",
    "treasury-analytics-service": "treasury",
    "treasury-compliance-service": "treasury",
    "treasury-management-service": "treasury",
    "treasury-policy-service": "treasury",
    "treasury-reporting-service": "treasury",
    "treasury-risk-service": "treasury",
    "tax-accounting-service": "taxation",
    "tax-compliance-service": "taxation",
    "tax-planning-service": "taxation",
    "biological-assets-service": "assets",
    "disposal-account-service": "assets",
    "disposal-group-service": "assets",
    "fixed-assets-register-service": "assets",
    "fixed-assets-schedule-service": "assets",
    "intangible-assets-service": "assets",
    "investment-property-service": "assets",
    "lease-termination-service": "assets",
    "revaluation-model-service": "assets",
    "authorized-share-capital-service": "equity",
    "ordinary-shares-service": "equity",
    "bonus-shares-service": "equity",
    "general-reserve-service": "equity",
    "retained-profits-service": "equity",
    "right-issues-service": "equity",
    "preference-shares-service": "equity",
    "bonus-shares-service": "equity",
    "general-reserve-service": "equity",
    "retained-profits-service": "equity",
    "capital-reconstruction-service": "equity",
    "capital-redemption-reserve-service": "equity",
    "debentures-service": "equity",
    "equity-changes-service": "equity",
    "general-reserve-service": "equity",
    "initial-investment-service": "equity",
    "issued-share-capital-service": "equity",
    "ordinary-shares-service": "equity",
    "bonus-shares-service": "equity",
    "general-reserve-service": "equity",
    "retained-profits-service": "equity",
    "right-issues-service": "equity",
    "preference-shares-service": "equity",
    "retained-profits-service": "equity",
    # an asset revaluation reserve is an asset question, and its records live in the assets crate
    "revaluation-reserve-service": "assets",
    "authorized-share-capital-service": "equity",
    "ordinary-shares-service": "equity",
    "bonus-shares-service": "equity",
    "general-reserve-service": "equity",
    "retained-profits-service": "equity",
    "right-issues-service": "equity",
    "preference-shares-service": "equity",
    "right-issues-service": "equity",
    "share-premium-service": "equity",
    "share-redemption-service": "equity",
    "family-community-group-service": "partnerships",
    "npo-scale-service": "partnerships",
    "npo-service": "partnerships",
    "partnership-agreement-service": "partnerships",
    "partnership-changes-service": "partnerships",
    "partnership-dissolution-service": "partnerships",
    "partnership-revaluation-service": "partnerships",
    "benefits-admin-service": "payroll",
    "dashboard-service": "reporting",
    "report-automation-service": "reporting",
    "report-distribution-service": "reporting",
    "reporting-service": "reporting",
    "data-warehouse-service": "analytics",
    "etl-service": "analytics",
    "performance-benchmarking-service": "analytics",
    "continue-shutdown-decision-service": "finance",
    "insurance-claims-service": "finance",
    "internal-rate-return-service": "finance",
    "investment-monitoring-service": "finance",
    "net-present-value-service": "finance",
    "payback-period-service": "finance",
    "personal-finance-service": "finance",
    "trade-finance-service": "finance",
    "audit-compliance-service": "audit",
    "audit-engagement-service": "audit",
    "audit-sampling-service": "audit",
    "regulatory-compliance-service": "compliance",
    "sox-compliance-service": "compliance",
    "risk-assessment-service": "risk",
    "distributed-workflow-orchestration-service": "workflow",
    "workflow-service": "workflow",
    "automation-engine-service": "automation",
    "multimodal-pipeline-service": "multimodal",
    "document-service": "documents",
    "graphql-service": "integrations",
    "webhook-service": "integrations",
    "calculation-api": "calculations",
    "realtime-calculation-engine": "calculations",
    "admin-service": "platform",
    "alerts-service": "platform",
    "book-sync-service": "platform",
    "cache-service": "platform",
    "encrypted-backup-service": "platform",
    "financial-identity-service": "platform",
    "message-bus-service": "platform",
    "mfa-auth-service": "platform",
    "notifications-service": "platform",
    "privacy-admin-dashboard-service": "platform",
    "websocket-service": "platform",
    "zero-trust-data-service": "platform",
}

PERSISTENCE = re.compile(r"neo4j|AsyncSession|sqlite3|crud\.")


def routed():
    with open(SERVICES, encoding="utf-8") as handle:
        return json.load(handle)["services"]


def ported_names():
    """The services the port tracker lists as wired to the Rust library.

    Only the bullets under "Wired to the Rust library" count. The tracker also lists the services
    whose maths the library already has but which are not served from it yet, and the ones still
    waiting for maths. Searching the whole file for the name counted those as ported - and it
    counted `currency-service` because `multi-currency-service` contains it, and
    `risk-assessment-service` because `fraud-risk-assessment-service` does. The estate was being
    reported as far more Rust than it is, which is the one thing this file exists to prevent.
    """
    listed = set()
    section = ""
    with open(PORT_MAP, encoding="utf-8") as handle:
        for line in handle:
            stripped = line.strip()
            if stripped.startswith("## "):
                section = stripped
                continue
            if "Wired to the Rust library" not in section:
                continue
            # a bullet names the service, then says where it lives - either "-> crate" or
            # "(crate, what it covers)". Reading only the arrow form silently dropped every service
            # annotated with a parenthetical, which is how two dozen Rust services were reported as
            # Python in the catalogue while answering from Rust.
            match = re.match(r"^-\s+([a-z0-9][a-z0-9._-]*)\s*(?:[(-].*)?$", stripped)
            if match:
                listed.add(match.group(1))
    return {name for name in OWNERS if name in listed}


def shape(name):
    """Endpoint count, and whether the service still owns records."""
    path = os.path.join(REPO, name, "main.py")
    if not os.path.isfile(path):
        return 0, False
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    routes = len(re.findall(r"@app\.(?:get|post|put|delete|patch)\(", text))
    return routes, bool(PERSISTENCE.search(text))


def render():
    services = routed()
    names = [service["name"] for service in services]
    by_name = {service["name"]: service for service in services}
    unowned = sorted(name for name in names if name not in OWNERS)
    stale = sorted(name for name in OWNERS if name not in names)
    ported = ported_names()

    lines = [
        "# Migration manifest",
        "",
        "Generated by `scripts/migration_manifest.py`. Every service the gateway routes has exactly",
        "one row here and exactly one owner domain. A routed service with no owner is reported below",
        "and fails the test, rather than quietly dropping out of the plan.",
        "",
        "The domain is the business responsibility. Where a service already runs on a Rust engine the",
        "row says `rust`; everything else is still Python and carries the gate it has to pass first.",
        "",
        "## Where things stand",
        "",
        "```",
        f"routed services          {len(names)}",
        f"already on a Rust engine {len([n for n in names if n in ported])}",
        f"still Python             {len([n for n in names if n not in ported])}",
        f"owner domains            {len(set(OWNERS.values()))}",
        "```",
        "",
    ]

    if unowned or stale:
        lines += ["## Needs attention", ""]
        lines += [f"- `{name}` is routed but has no owner domain" for name in unowned]
        lines += [f"- `{name}` has an owner but is no longer routed" for name in stale]
        lines += [""]

    for domain in sorted(set(OWNERS.values())):
        members = [name for name in names if OWNERS.get(name) == domain]
        if not members:
            continue
        purpose, destination = DOMAINS[domain]
        lines += [f"## {domain}", "", f"_{purpose}_", "", f"Lands in `{destination}`.", ""]
        lines.append("```")
        lines.append(f"{'service':<46}{'port':>6}{'routes':>8}{'records':>9}  engine")
        for name in sorted(members):
            routes, owns = shape(name)
            engine = "rust" if name in ported else "python"
            records = "yes" if owns else "no"
            lines.append(f"{name:<46}{by_name[name]['port']:>6}{routes:>8}{records:>9}  {engine}")
        lines.append("```")
        lines.append("")

        if any(name in ported for name in members):
            lines.append(
                "Retirement gate: answered from Rust with parity holding, so the Python directory "
                "goes once that gateway route is switched."
            )
        if any(name not in ported for name in members):
            lines.append(
                "Retirement gate: port the maths and its records into this domain, capture parity "
                "against the Python, then de-register. Nothing is deleted before that."
            )
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def main():
    body = render()
    with open(OUT, "w", encoding="utf-8") as handle:
        handle.write(body)
    names = {service["name"] for service in routed()}
    ported = ported_names()
    print(
        f"wrote MIGRATION-MANIFEST.md: {len(names)} routed, "
        f"{len(names & ported)} on rust, {len(names - ported)} python"
    )
    missing = sorted(names - set(OWNERS))
    if missing:
        print("unowned:", ", ".join(missing))


if __name__ == "__main__":
    main()
