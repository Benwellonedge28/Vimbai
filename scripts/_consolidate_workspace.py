"""One-shot: fold library/ and ledger-core/ into a single rust/ workspace.

Run from the repository root. Moves with `git mv` so history follows each file,
rewrites every path dependency to its new relative location, and leaves the crate
names alone: this is a structural move, not a rewrite.
"""

import os
import re
import subprocess
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


def move(src: Path, dst: Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "mv", str(src), str(dst)], check=True)


def main() -> None:
    moves: dict[Path, Path] = {}
    for name in PLATFORM_CRATES:
        moves[Path("library") / name] = Path("rust/crates") / name
    for name in CALCULATION_CRATES:
        moves[Path("library") / name] = Path("rust/crates/calculations") / name
    moves[Path("library/native")] = Path("rust/native")
    moves[Path("ledger-core")] = Path("rust/crates/kernel/ledger-core")

    # Every directory that holds a Cargo.toml, before and after.
    new_of = {old.resolve(): (root / new).resolve() for old, new in moves.items()}

    # Workspace-level files first, so cargo never sees a workspace with missing members.
    move(Path("library/Cargo.toml"), Path("rust/Cargo.toml"))
    move(Path("library/Cargo.lock"), Path("rust/Cargo.lock"))
    for doc in sorted(Path("library").glob("*.md")):
        move(doc, Path("rust") / doc.name)
    for old, new in moves.items():
        move(old, new)
    Path("library").rmdir()

    # Rewrite path dependencies from the old layout into the new one.
    for old_dir, new_dir in new_of.items():
        manifest = new_dir / "Cargo.toml"
        if not manifest.exists():
            continue
        text = manifest.read_text()
        if (old_dir / "Cargo.toml").exists():
            before = (old_dir / "Cargo.toml").read_text()
        else:
            before = text

        def fix(match: re.Match[str]) -> str:
            rel = match.group(1)
            target = (old_dir / rel).resolve()
            if target not in new_of:
                # Not a crate that moved: recompute it relative to the new home.
                landed = (new_dir / rel).resolve()
                return f'path = "{os.path.relpath(landed, new_dir)}"'
            moved_to = new_of[target]
            return f'path = "{os.path.relpath(moved_to, new_dir)}"'

        updated = re.sub(r'path = "([^"]+)"', fix, before)
        # ledger-core is a workspace member now, so its own profile section is dead weight.
        if new_dir.name == "ledger-core":
            updated = re.sub(r"\n\[profile\.release\]\n(?:[^\[]*)", "\n", updated)
        manifest.write_text(updated)

    # The workspace root learns its new member layout.
    workspace = Path("rust/Cargo.toml")
    members = ["crates/common", "crates/api", "crates/store", "crates/bridge", "crates/automation"]
    members += ["crates/sso", "crates/rbac", "crates/abac"]
    members += ["crates/kernel/ledger-core"]
    members += [f"crates/calculations/{name}" for name in CALCULATION_CRATES]
    text = workspace.read_text()
    block = "members = [\n" + "".join(f'    "{member}",\n' for member in members) + "]"
    text = re.sub(r"members = \[[^\]]*\]", block, text, count=1)
    workspace.write_text(text)

    print(f"moved {len(moves)} crates into rust/, {len(members)} workspace members")


if __name__ == "__main__":
    main()
