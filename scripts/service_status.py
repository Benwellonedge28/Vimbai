#!/usr/bin/env python3
"""Generate the service status index and per-service status markers.

The repository keeps every service directory, including the calculators whose
figures now come from the Rust library crates. That is deliberate: the Python
maths is the reference implementation the Rust ports are parity-tested against,
and the retired services keep their regression coverage in CI.

What was missing was a way to tell, at a glance, which services are live and
which are retained source. This script derives that from the authoritative
sources - the gateway route table and the retirement families pinned in
tests/test_service_dedup.py - and writes:

    SERVICE-STATUS.md            the index, grouped by status
    <service>/STATUS.md          a one-screen marker inside each retained service

Nothing else is touched, and no service directory is ever removed.

Usage:
    python3 scripts/service_status.py            rewrite the index and markers
    python3 scripts/service_status.py --check    exit 1 if anything is stale
"""

import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEX = os.path.join(ROOT, "SERVICE-STATUS.md")

# Family name -> (human label, Rust crate that owns the figures now).
FAMILIES = {
    "CALCULATOR_PORTS": ("treasury calculator", "treasury_fx"),
    "COST_CALCULATOR_PORTS": ("cost accounting calculator", "cost_accounting"),
    "VARIANCE_BUDGET_PORTS": ("variance / budget calculator", None),
    "SCENARIO_BUDGET_PORTS": ("scenario budgeting calculator", "budgeting"),
    "IFRS_CALCULATOR_PORTS": ("IFRS calculator", "ifrs"),
    "STATEMENT_PAYROLL_PORTS": ("statement / payroll transposition", None),
    "HEDGING_FX_PORTS": ("hedging, FX and reporting calculator", "treasury_fx"),
    "CAPITAL_RETURN_PORTS": ("capital return calculator", "capital_valuation"),
    "VALUATION_PORTS": ("business valuation calculator", "capital_valuation"),
    "CORPORATE_EVENT_PORTS": ("corporate event calculator", "capital_valuation"),
    "REPORT_ANALYSIS_PORTS": ("reporting diagnostic", "reporting"),
    "LEDGER_ANALYTICS_PORTS": ("ledger study endpoint", "ledger-core"),
    "ADVISORY_PORTS": ("advisory endpoint", "advisory"),
    "INVESTOR_METRIC_PORTS": ("investor metric calculator", "ratios_statements"),
    "FIXED_ASSETS_PORTS": ("fixed asset calculator", None),
    "TAX_PORTS": ("tax calculator", "tax"),
    "ABSORPTION_MARGIN_PORTS": ("absorption / margin calculator", "cost_accounting"),
    "RISK_DUPLICATES": ("duplicate of risk-assessment-service", None),
    "PLATFORM_TEMPLATE_DUPLICATES": ("platform template duplicate", None),
}

# Candidate crates per family, best guess first. The service slug is looked for
# in each candidate's source; failing that, the first candidate is used.
FAMILY_CANDIDATES = {
    "CALCULATOR_PORTS": ["treasury_fx"],
    "COST_CALCULATOR_PORTS": ["cost_accounting"],
    "ABSORPTION_MARGIN_PORTS": ["cost_accounting"],
    "VARIANCE_BUDGET_PORTS": ["variance", "budgeting"],
    "SCENARIO_BUDGET_PORTS": ["budgeting"],
    "IFRS_CALCULATOR_PORTS": ["ifrs"],
    "STATEMENT_PAYROLL_PORTS": ["ratios_statements", "equity_payroll"],
    "HEDGING_FX_PORTS": ["treasury_fx", "reporting"],
    "CAPITAL_RETURN_PORTS": ["capital_valuation"],
    "VALUATION_PORTS": ["capital_valuation"],
    "CORPORATE_EVENT_PORTS": ["capital_valuation"],
    "REPORT_ANALYSIS_PORTS": ["reporting"],
    "LEDGER_ANALYTICS_PORTS": ["ledger-core"],
    "ADVISORY_PORTS": ["advisory"],
    "INVESTOR_METRIC_PORTS": ["ratios_statements", "capital_valuation"],
    "FIXED_ASSETS_PORTS": ["ratios_statements"],
    "TAX_PORTS": ["tax"],
    "RETIRED_PORTS": ["performance_measures"],
}

BUDGET_NAMES = ("budget",)


def _parse_families():
    """Read the retirement families pinned in the dedup regression tests."""
    path = os.path.join(ROOT, "tests", "test_service_dedup.py")
    source = open(path, encoding="utf-8").read()
    families = {}
    for match in re.finditer(r"^([A-Z][A-Z0-9_]*)\s*=\s*\{", source, re.M):
        name = match.group(1)
        if "PORT" not in name and "DUPLICATE" not in name:
            continue
        depth, i = 1, match.end()
        while depth:
            char = source[i]
            depth += 1 if char == "{" else (-1 if char == "}" else 0)
            i += 1
        for service in re.findall(r'"([^"]+)"', source[match.end() : i]):
            families.setdefault(service, []).append(name)
    return families


def _gateway_routes():
    path = os.path.join(ROOT, "api-gateway", "config", "services.json")
    payload = json.load(open(path, encoding="utf-8"))
    items = payload if isinstance(payload, list) else payload.get("services", [])
    return {(item["name"] if isinstance(item, dict) else item) for item in items}


def _crate_corpus():
    corpus = {}
    for crate in sorted(os.listdir(os.path.join(ROOT, "library"))):
        base = os.path.join(ROOT, "library", crate)
        src = os.path.join(base, "src")
        if not os.path.isdir(src) or not os.path.isfile(os.path.join(base, "Cargo.toml")):
            continue
        text = ""
        for path in glob.glob(os.path.join(src, "**", "*.rs"), recursive=True):
            text += open(path, encoding="utf-8", errors="ignore").read()
        corpus[crate] = text
    return corpus


def _crate_for(service, families, corpus):
    slug = service[: -len("-service")] if service.endswith("-service") else service
    for family in families:
        candidates = FAMILY_CANDIDATES.get(family, [])
        for crate in candidates:
            if slug in corpus.get(crate, ""):
                return crate
        if candidates:
            return candidates[0]
    return None


def _service_dirs(routes):
    """Service directories: the -service convention, plus any bare routed name.

    A couple of engines are mounted under their bare name
    (org-authorization-engine, realtime-calculation-engine), so the route table
    is consulted too.
    """
    return sorted(
        name
        for name in os.listdir(ROOT)
        if os.path.isdir(os.path.join(ROOT, name)) and (name.endswith("-service") or name in routes)
    )


def classify():
    routes = _gateway_routes()
    families = _parse_families()
    corpus = _crate_corpus()
    live, retained, unclassified = [], {}, []
    for service in _service_dirs(routes):
        if service in routes:
            live.append(service)
            continue
        if service in families:
            family = families[service][0]
            label, _ = FAMILIES.get(family, (family, None))
            retained[service] = {
                "family": family,
                "label": label,
                "crate": _crate_for(service, families[service], corpus),
            }
            continue
        unclassified.append(service)
    return live, retained, unclassified


def _marker(service, info):
    if info["family"].endswith("DUPLICATES"):
        return f"""# Status: de-registered duplicate

`{service}` was an exact clone ({info["label"]}) and is no longer routed by the
gateway.

The source stays in the repository under the standing no-deletion rule, and so
that the history of the clone family is readable in place. Nothing imports it,
nothing deploys it, and nothing tests it.

Do not delete this directory. This file is generated by
`scripts/service_status.py`.
"""

    crate = info["crate"]
    crate_line = (
        f"Its figures now come from the Rust library crate `{crate}` " f"(`library/{crate}/`)."
        if crate
        else "Its figures now come from the Rust calculation library."
    )
    return f"""# Status: retired from the gateway, source retained

This service is de-registered: it has no route in
`api-gateway/config/services.json` and is not deployed. {crate_line}

Why the Python stays: it is the reference implementation the Rust port is
parity-tested against, and its own regression tests remain wired into CI
(`.github/workflows/test-core.yml`). Deleting it would remove the oracle, not
just the old code.

Do not delete this directory. This file is generated by
`scripts/service_status.py`.
"""


def render(live, retained, unclassified):
    lines = [
        "# Service status index",
        "",
        "Generated by `scripts/service_status.py`. Do not edit by hand; rerun the",
        "script after registering, retiring or deduplicating a service.",
        "",
        "Retired services are deleted from the repository. Their maths lives in the Rust crate",
        "named beside each group, and the recorded parity captures in library/api/tests/parity are",
        "what the ports are tested against.",
        "",
        "## Summary",
        "",
        "```",
        f"live and routed          {len(live)}",
        f"retained, not routed     {len(retained)}",
        f"unrouted, undecided      {len(unclassified)}",
        "```",
        "",
        "## Live services",
        "",
        "Routed by the API gateway and deployed. These are the running product.",
        "",
    ]
    lines += [f"`{name}`" for name in live]
    lines += ["", "## Retained source (retired from routing)", ""]

    groups = {}
    for service, info in retained.items():
        groups.setdefault((info["label"], info["crate"]), []).append(service)
    for (label, crate), services in sorted(groups.items(), key=lambda kv: (kv[0][0], kv[0][1] or "")):
        crate_text = f" -> `{crate}`" if crate else ""
        if "duplicate" in label:
            note = f"{len(services)} services. De-registered as clones; source kept under the no-deletion rule."
        else:
            note = f"{len(services)} services. Source kept as the Rust parity reference and CI oracle."
        lines += [f"### {label}{crate_text}", "", note, ""]
        lines += [f"`{name}`" for name in sorted(services)]
        lines.append("")

    if unclassified:
        lines += [
            "## Unrouted, undecided",
            "",
            "Not in the gateway and not part of any retirement family. Each needs a",
            "decision: register it, retire it, or deduplicate it.",
            "",
        ]
        lines += [f"`{name}`" for name in unclassified]
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def build():
    live, retained, unclassified = classify()
    files = {INDEX: render(live, retained, unclassified)}
    for service, info in retained.items():
        files[os.path.join(ROOT, service, "STATUS.md")] = _marker(service, info)
    return files, retained


def main(argv):
    files, retained = build()
    check = "--check" in argv
    stale = []
    for path, content in files.items():
        current = open(path, encoding="utf-8").read() if os.path.isfile(path) else None
        if current != content:
            stale.append(os.path.relpath(path, ROOT))
            if not check:
                open(path, "w", encoding="utf-8").write(content)
    markers = {os.path.join(ROOT, s, "STATUS.md") for s in retained}
    for path in glob.glob(os.path.join(ROOT, "*-service", "STATUS.md")):
        if path not in markers:
            stale.append(os.path.relpath(path, ROOT))
            if not check:
                os.remove(path)
    if check:
        if stale:
            print("stale status files:")
            for name in sorted(stale):
                print("  " + name)
            print("run: python3 scripts/service_status.py")
            return 1
        print("service status index is current")
        return 0
    print(f"wrote {os.path.relpath(INDEX, ROOT)} and {len(retained)} service markers")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
