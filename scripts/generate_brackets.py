#!/usr/bin/env python3
"""
Vimbai bracket-service generator.

Groups the ~330 single-file FastAPI microservices into a small number of
"bracket" services (one container per bracket). Each bracket mounts every
member service's FastAPI app under its original gateway path prefix, so all
external URLs stay exactly the same.

Outputs (deterministic; safe to re-run):
  - brackets/<name>/{main.py, Dockerfile, requirements.txt, members.json}
  - api-gateway/config/services.json   (merged routes -> bracket URL, strip_prefix=false)
  - docker-compose.yml                 (merged compose blocks -> bracket blocks)
  - k8s/helm/vimbai/values.yaml        (newServices -> bracket deployments)

Usage: python3 scripts/generate_brackets.py [--dry-run]
"""

import json
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DRY_RUN = "--dry-run" in sys.argv

SERVICES_JSON = os.path.join(REPO, "api-gateway", "config", "services.json")
COMPOSE = os.path.join(REPO, "docker-compose.yml")
VALUES = os.path.join(REPO, "k8s", "helm", "vimbai", "values.yaml")
BRACKETS_DIR = os.path.join(REPO, "brackets")

# Services that stay standalone (real DBs / message bus / core platform):
STANDALONE = {
    "accounting-service",
    "finance-service",
    "workflow-service",
    "message-bus-service",
    "multimodal-pipeline-service",
    "document-service",
    "invoicing-service",
    "pos-integration-service",
    "scenario-modeling-service",
    "npo-service",
    "banking-integration-service",
    "supply-chain-service",
    "fraud-detection-service",
}

# Ordered bracket rules: first matching rule wins.
BRACKETS = [
    (
        "costing-budgeting-bracket",
        9001,
        [
            "costing",
            "cost-service",
            "standard-cost",
            "budget",
            "scorecard",
            "appropriation",
            "variance",
            "break-even",
            "breakeven",
        ],
    ),
    (
        "statements-reporting-bracket",
        9002,
        [
            "statement",
            "report",
            "trial-balance",
            "ledger",
            "balance-sheet",
            "annual-report",
            "chart-of-accounts",
            "financial-position",
        ],
    ),
    (
        "treasury-banking-bracket",
        9003,
        [
            "treasury",
            "bank",
            "amortization",
            "acquisition-financing",
            "cash-flow",
            "share-capital",
            "dividend",
            "loan",
            "credit",
            "interest",
            "factoring",
            "leasing",
            "debt",
            "financing",
            "payment",
            "investment",
            "currency",
            "forex",
        ],
    ),
    (
        "tax-audit-investigation-bracket",
        9004,
        [
            "tax",
            "audit",
            "forensic",
            "investigation",
            "compliance",
            "assurance",
            "attestation",
        ],
    ),
    (
        "risk-governance-bracket",
        9005,
        [
            "risk",
            "fraud",
            "governance",
            "esg",
            "sustainability",
            "internal-control",
            "whistleblow",
        ],
    ),
    (
        "ap-ar-expenses-bracket",
        9006,
        [
            "payable",
            "receivable",
            "expense",
            "bad-debts",
            "payroll",
            "benefits",
            "employee",
            "salary",
            "reimburse",
            "pension",
            "gratuity",
            "leave",
        ],
    ),
    (
        "ratios-analytics-bracket",
        9007,
        [
            "ratio",
            "turnover",
            "rate-return",
            "analytic",
            "ai-readiness",
            "asset-allocation",
            "benchmark",
            "kpi",
            "metric",
            "forecasting",
            "modeling",
            "prediction",
        ],
    ),
    (
        "platform-automation-bracket",
        9008,
        [
            "alert",
            "notification",
            "admin",
            "automation",
            "schedule",
            "webhook",
            "sync",
            "search",
            "feed",
            "integration",
            "gateway",
            "config",
            "feature-flag",
        ],
    ),
    (
        "operations-inventory-bracket",
        9009,
        [
            "supply-chain",
            "inventory",
            "procurement",
            "logistics",
            "warehouse",
            "manufacturing",
            "production",
            "sales",
            "order",
            "billing",
            "subscription",
            "retail",
            "ecommerce",
            "e-commerce",
            "fixed-asset",
            "asset-management",
            "biological",
            "consignment",
            "project",
            "job-cost",
            "hr-",
            "recruitment",
        ],
    ),
    (
        "advanced-accounting-bracket",
        9011,
        [
            "accounting",
            "apportionment",
            "bonus-shares",
            "consolidation",
            "construction-contract",
            "control-account",
            "cost-centre",
            "depreciation",
            "director-emoluments",
            "disposal",
            "double-entry",
            "equity-changes",
            "financial-identity",
            "financial-integrity",
            "cashbook",
            "equivalent-units",
            "goodwill",
            "government-grants",
            "ifrs",
            "intangible-assets",
            "intercompany",
            "journal-entries",
            "lease-accounting",
            "lease-management",
            "lease-termination",
            "management-accounts",
            "net-realizable-value",
            "ordinary-shares",
            "over-under-absorption",
            "overhead",
            "partnership",
            "petty-cash",
            "preference-shares",
            "profit-loss-account",
            "profit-service",
            "provisions-contingencies",
            "related-party",
            "retained-profits",
            "revaluation",
            "right-issues",
            "revenue-recognition",
            "share-options",
            "share-premium",
            "share-redemption",
            "statutory-filing",
            "substantive-testing",
            "suspense-error",
            "throughput-accounting",
            "trading-account",
            "transfer-pricing",
            "weighted-average",
            "general-reserve",
            "going-concern",
            "fund-accounting",
            "continue-shutdown-decision",
        ],
    ),
    (
        "corporate-finance-bracket",
        9012,
        [
            "valuation",
            "capital",
            "cash-management",
            "cash-optimization",
            "cash-pooling",
            "hedging",
            "cost-of-capital",
            "croic",
            "cvp-analysis",
            "deal-structuring",
            "debentures",
            "discount",
            "divestiture",
            "due-diligence",
            "eps-service",
            "eva-service",
            "derivatives",
            "financial-planning",
            "foreign-exchange",
            "futures",
            "household-finance",
            "insurance-claims",
            "limiting-factor",
            "liquidity",
            "make-or-buy",
            "margin-safety",
            "merger",
            "mva-service",
            "net-present-value",
            "options-pricing",
            "payback-period",
            "personal-finance",
            "portfolio-optimization",
            "post-merger",
            "present-value",
            "profitability-index",
            "rolling-forecast",
            "scenario-analysis",
            "sensitivity-analysis",
            "sustainable-growth",
            "synergy-analysis",
            "time-value-of-money",
            "trade-finance",
            "working-capital",
            "fund-management",
        ],
    ),
    (
        "platform-infrastructure-bracket",
        9013,
        [
            "cache",
            "cqrs",
            "dashboard",
            "data-sovereignty",
            "disaster-recovery",
            "edge-computing",
            "encrypted-backup",
            "enterprise-sso",
            "etl",
            "event-streaming",
            "federated",
            "financial-state-machine",
            "graphql",
            "grpc",
            "high-availability",
            "infrastructure-as-code",
            "mfa-auth",
            "multi-cloud",
            "multi-tenant",
            "observability",
            "plugin-extension",
            "policy-engine",
            "realtime-calculation-engine",
            "websocket",
            "zero-trust",
        ],
    ),
]
CATCH_ALL = ("general-services-bracket", 9010)

BRACKET_PORT = {b[0]: b[1] for b in BRACKETS}
BRACKET_PORT[CATCH_ALL[0]] = CATCH_ALL[1]
BRACKET_TITLE = {
    "costing-budgeting-bracket": "Costing & Budgeting",
    "statements-reporting-bracket": "Statements & Reporting",
    "treasury-banking-bracket": "Treasury & Banking",
    "tax-audit-investigation-bracket": "Tax, Audit & Investigation",
    "risk-governance-bracket": "Risk & Governance",
    "ap-ar-expenses-bracket": "AP/AR, Payroll & Expenses",
    "ratios-analytics-bracket": "Ratios & Analytics",
    "platform-automation-bracket": "Platform & Automation",
    "operations-inventory-bracket": "Operations & Inventory",
    "operations-inventory-bracket": "Operations & Inventory",
    "advanced-accounting-bracket": "Advanced Accounting",
    "corporate-finance-bracket": "Corporate Finance",
    "platform-infrastructure-bracket": "Platform Infrastructure",
    "general-services-bracket": "General Services",
}

MAIN_TEMPLATE = '''"""
Vimbai %(title)s Bracket Service
One container hosting %(count)d merged microservices. Each member service's
FastAPI app is mounted at its original gateway path prefix, so every
external URL is unchanged.

GENERATED by scripts/generate_brackets.py - do not edit by hand;
re-run the generator instead.
"""

import importlib.util
import os
import sys

from fastapi import FastAPI

SERVICE_NAME = "%(slug)s"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "%(port)d"))

# (gateway path prefix, service directory) pairs
MEMBERS = %(members)s

app = FastAPI(
    title="Vimbai %(title)s Bracket Service",
    version=SERVICE_VERSION,
    description="Bracket container for %(count)d Vimbai microservices.",
)

_HERE = os.path.dirname(os.path.abspath(__file__))

_loaded = 0
for _prefix, _svc_dir in MEMBERS:
    # Repo layout: brackets/<name>/main.py with members at repo root.
    _svc_path = os.path.normpath(os.path.join(_HERE, "..", "..", _svc_dir, "main.py"))
    if not os.path.isfile(_svc_path):
        # Docker layout: /app/main.py with members copied next to it.
        _svc_path = os.path.join(_HERE, _svc_dir, "main.py")
    _mod_name = "vimbai_bracket_" + _svc_dir.replace("-", "_")
    _spec = importlib.util.spec_from_file_location(_mod_name, _svc_path)
    _mod = importlib.util.module_from_spec(_spec)
    # Register before exec: some libs (e.g. strawberry) inspect
    # sys.modules[cls.__module__] during class definition.
    sys.modules[_mod_name] = _mod
    _spec.loader.exec_module(_mod)
    app.mount("/" + _prefix.lstrip("/"), _mod.app)
    _loaded += 1


@app.get("/health")
def bracket_health():
    return {
        "service": SERVICE_NAME,
        "status": "healthy",
        "mounted_services": _loaded,
    }


@app.get("/")
def bracket_root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "mounted_services": _loaded,
        "paths": ["/" + p for p, _ in MEMBERS],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
'''

DOCKERFILE_TEMPLATE = """# GENERATED by scripts/generate_brackets.py - do not edit by hand.
# Build context must be the repository root:
#   docker build -f brackets/%(slug)s/Dockerfile .
FROM python:3.11-slim

ENV PYTHONUNBUFFERED=1 \\
    PYTHONDONTWRITEBYTECODE=1

WORKDIR /app

# Non-root user
RUN addgroup --system vimbai && adduser --system --ingroup vimbai vimbai

COPY brackets/%(slug)s/requirements.txt ./requirements.txt
RUN pip install --no-cache-dir --upgrade pip \\
    && pip install --no-cache-dir -r requirements.txt

COPY brackets/%(slug)s/main.py ./main.py
%(copy_lines)s

USER vimbai

EXPOSE %(port)d

CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "%(port)d"]
"""


def merged_requirements(member_dirs):
    seen = {}
    for d in member_dirs:
        req = os.path.join(REPO, d, "requirements.txt")
        if not os.path.isfile(req):
            continue
        for line in open(req, encoding="utf-8", errors="ignore"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            pkg = re.split(r"[<>=!~;\[]", line, 1)[0].strip().lower()
            if pkg and pkg not in seen:
                seen[pkg] = line
    return sorted(seen.values())


def assign_bracket(name):
    for bracket, _port, rules in BRACKETS:
        if any(r in name for r in rules):
            return bracket
    return CATCH_ALL[0]


def main():
    services = json.load(open(SERVICES_JSON))["services"]

    brackets = {}
    for svc in services:
        name = svc["name"]
        if name in STANDALONE:
            continue
        if not os.path.isdir(os.path.join(REPO, name)):
            print(f"  !! service dir missing for {name}, skipping")
            continue
        slug = assign_bracket(name)
        brackets.setdefault(slug, []).append((svc["path"].lstrip("/"), name, name))

    total = sum(len(v) for v in brackets.values())
    print(f"Merging {total} services into {len(brackets)} brackets:")
    for slug in sorted(brackets):
        print(f"  {slug}: {len(brackets[slug])}")

    if DRY_RUN:
        return

    for slug, members in brackets.items():
        port = BRACKET_PORT[slug]
        bdir = os.path.join(BRACKETS_DIR, slug)
        os.makedirs(bdir, exist_ok=True)
        members_sorted = sorted(members)

        member_pairs = [(p, d) for p, _n, d in members_sorted]
        main_src = MAIN_TEMPLATE % {
            "title": BRACKET_TITLE[slug],
            "slug": slug,
            "port": port,
            "count": len(members_sorted),
            "members": json.dumps(member_pairs, indent=4),
        }
        open(os.path.join(bdir, "main.py"), "w").write(main_src)

        copy_lines = "\n".join(f"COPY {d} ./{d}" for _p, _n, d in members_sorted)
        open(os.path.join(bdir, "Dockerfile"), "w").write(
            DOCKERFILE_TEMPLATE % {"slug": slug, "port": port, "copy_lines": copy_lines}
        )

        reqs = merged_requirements([d for _p, _n, d in members_sorted])
        open(os.path.join(bdir, "requirements.txt"), "w").write("\n".join(reqs) + "\n")

        json.dump(
            {
                "bracket": slug,
                "port": port,
                "members": [{"path": "/" + p, "name": n, "dir": d} for p, n, d in members_sorted],
            },
            open(os.path.join(bdir, "members.json"), "w"),
            indent=2,
        )

    # --- services.json ---
    bracket_url = {s: "http://localhost:%d" % BRACKET_PORT[s] for s in brackets}
    bracket_of = {}
    for slug, members in brackets.items():
        for _p, n, _d in members:
            bracket_of[n] = slug

    for svc in services:
        slug = bracket_of.get(svc["name"])
        if slug:
            svc["url"] = bracket_url[slug]
            svc["strip_prefix"] = False

    with open(SERVICES_JSON, "w") as f:
        json.dump({"services": services}, f, indent=2)
        f.write("\n")
    print(f"services.json rewritten ({len(services)} routes)")

    # --- docker-compose.yml ---
    import yaml

    compose = yaml.safe_load(open(COMPOSE))
    svc_map = compose.get("services", {})
    merged_names = {n for sl in brackets.values() for _p, n, _d2 in sl}

    keep = {}
    dropped = 0
    for key, block in svc_map.items():
        d = key.replace("_", "-")
        if d in merged_names:
            dropped += 1
            continue
        keep[key] = block

    for slug in sorted(brackets):
        port = BRACKET_PORT[slug]
        keep[slug.replace("-", "_")] = {
            "build": {"context": ".", "dockerfile": "brackets/%s/Dockerfile" % slug},
            "ports": ["%d:%d" % (port, port)],
            "environment": [
                "JWT_SECRET=${JWT_SECRET:-dev-secret}",
                "NEO4J_URI=${NEO4J_URI:-bolt://neo4j:7687}",
                "NEO4J_USER=${NEO4J_USER:-neo4j}",
                "NEO4J_PASSWORD=${NEO4J_PASSWORD:-dev-password}",
                "REDIS_URL=${REDIS_URL:-redis://redis:6379}",
            ],
            "restart": "unless-stopped",
            "networks": ["vimbai-net"],
        }

    compose["services"] = dict(sorted(keep.items()))
    header = """# Vimbai - Full Stack Docker Compose
# 330+ single-file services are consolidated into bracket containers
# (brackets/) to reduce container count and cost. Generated by
# scripts/generate_brackets.py - re-run it after adding new services.
# Usage: docker compose up -d (or docker compose up <service_name>)
# Health checks: docker compose ps

"""
    with open(COMPOSE, "w") as f:
        f.write(header)
        yaml.safe_dump(compose, f, default_flow_style=False, sort_keys=False)
    print(
        f"docker-compose.yml rewritten: {dropped} blocks -> {len(brackets)} brackets, "
        f"kept {len(keep) - len(brackets)} standalone"
    )

    # --- k8s values newServices ---
    val_src = open(VALUES).read()
    indent = "  "
    blocks = []
    for slug in sorted(brackets):
        port = BRACKET_PORT[slug]
        blocks.append(
            f'{indent}- name: "{slug}"\n'
            f"{indent}  port: {port}\n"
            f"{indent}  replicas: 2\n"
            f"{indent}  maxReplicas: 6\n"
            f"{indent}  resources:\n"
            f"{indent}    requests:\n"
            f'{indent}      memory: "512Mi"\n'
            f'{indent}      cpu: "250m"\n'
            f"{indent}    limits:\n"
            f'{indent}      memory: "1Gi"\n'
            f'{indent}      cpu: "1"\n'
        )
    new_block = "newServices:\n" + "\n".join(blocks) + "\n"
    val_new = re.sub(r"(?ms)^newServices:\n.*\Z", new_block, val_src)
    if "newServices" not in val_src:
        val_new = val_src.rstrip("\n") + "\n\n" + new_block
    if val_new == val_src:
        print("k8s values.yaml: newServices already up to date")
    else:
        open(VALUES, "w").write(val_new)
        print(f"k8s values.yaml: newServices -> {len(brackets)} bracket deployments")


if __name__ == "__main__":
    main()
