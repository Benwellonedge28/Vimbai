"""One-shot: rewrite every path dependency, member list and ignore file for rust/.

Companion to _consolidate_workspace.py, which moved the directories with `git mv`.
Run from the repository root after the move.
"""

import os
import re
from pathlib import Path

CALCULATION_CRATES = [
    "advisory",
    "assets",
    "assurance",
    "budgeting",
    "capital_valuation",
    "cost_accounting",
    "equity_payroll",
    "group_accounting",
    "ifrs",
    "market_risk",
    "performance_measures",
    "ratios_statements",
    "reporting",
    "tax",
    "tax_reporting",
    "treasury_fx",
    "variance",
]
PLATFORM_CRATES = ["common", "api", "store", "bridge", "automation", "sso", "rbac", "abac"]

root = Path.cwd()

old_of: dict[Path, Path] = {}
for name in PLATFORM_CRATES:
    old_of[(root / "rust/crates" / name).resolve()] = (root / "library" / name).resolve()
for name in CALCULATION_CRATES:
    old_of[(root / "rust/crates/calculations" / name).resolve()] = (root / "library" / name).resolve()
old_of[(root / "rust/native").resolve()] = (root / "library/native").resolve()
old_of[(root / "rust/crates/kernel/ledger-core").resolve()] = (root / "ledger-core").resolve()

new_of = {old: new for new, old in old_of.items()}


def rewrite(manifest: Path) -> int:
    new_dir = manifest.parent.resolve()
    old_dir = old_of.get(new_dir, new_dir)
    text = manifest.read_text()
    changed = 0

    def fix(match: re.Match[str]) -> str:
        nonlocal changed
        rel = match.group(1)
        old_target = (old_dir / rel).resolve()
        if old_target in new_of:
            changed += 1
            return f'path = "{os.path.relpath(new_of[old_target], new_dir)}"'
        # A path that stayed put keeps its meaning only if the target still exists.
        if (new_dir / rel).exists():
            return match.group(0)
        moved = (new_dir / rel).resolve()
        changed += 1
        return f'path = "{os.path.relpath(moved, new_dir)}"'

    manifest.write_text(re.sub(r'path = "([^"]+)"', fix, text))
    return changed


total = 0
for new_dir in [p.parent for p in (root / "rust").rglob("Cargo.toml")]:
    if "target" in new_dir.parts:
        continue
    total += rewrite(new_dir / "Cargo.toml")
print(f"rewrote {total} path dependencies")

ledger = root / "rust/crates/kernel/ledger-core/Cargo.toml"
text = ledger.read_text()
ledger.write_text(re.sub(r"\n\[profile\.release\]\n(?:[^\[]*)", "\n", text))

workspace = root / "rust/Cargo.toml"
members = ["crates/common", "crates/api", "crates/store", "crates/bridge", "crates/automation"]
members += ["crates/sso", "crates/rbac", "crates/abac", "crates/kernel/ledger-core"]
members += [f"crates/calculations/{name}" for name in CALCULATION_CRATES]
block = "members = [\n" + "".join(f'    "{member}",\n' for member in members) + "]"
text = workspace.read_text()
text = re.sub(r"members = \[[^\]]*\]", block, text, count=1)
workspace.write_text(text)
print(f"{len(members)} workspace members")
