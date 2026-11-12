"""Retire a service whose maths now lives in Rust.

Removing a service from the gateway, the manifests and CI by hand is where a retirement goes wrong:
one leftover line keeps a container alive, and the container keeps answering from code nobody
maintains. This does the four edits together and reports what it touched, so a batch of services is
retired the same way every time.

Files are kept unless --delete-files is given, because a retired service's Python is still evidence:
the parity captures under rust/crates/api/tests/parity were taken from it. Deleting the directory is a
separate, deliberate act.

Usage: python3 scripts/retire_service.py NAME [NAME ...] [--delete-files] [--dry-run]
"""

import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "gateway" / "go" / "config" / "services.json"
MANIFESTS = (
    ROOT / "infrastructure" / "kubernetes" / "all-services.yaml",
    ROOT / "k8s" / "vimbai-all-services.yaml",
)
WORKFLOWS = ROOT / ".github" / "workflows"


def drop_from_gateway(names):
    payload = json.loads(MANIFEST.read_text())
    before = len(payload["services"])
    payload["services"] = [entry for entry in payload["services"] if entry["name"] not in names]
    removed = before - len(payload["services"])
    MANIFEST.write_text(json.dumps(payload, indent=2) + "\n")
    return removed


def drop_from_manifests(names):
    removed = 0
    for manifest in MANIFESTS:
        if not manifest.exists():
            continue
        documents = manifest.read_text().split("\n---\n")
        kept = []
        for document in documents:
            name = re.search(r"^  name: ([a-z0-9][a-z0-9-]*)\s*$", document, re.MULTILINE)
            if name and name.group(1) in names:
                removed += 1
                continue
            kept.append(document)
        manifest.write_text("\n---\n".join(kept))
    return removed


def drop_from_workflows(names):
    removed = 0
    for workflow in sorted(WORKFLOWS.glob("*.yml")):
        lines = workflow.read_text().splitlines(keepends=True)
        kept = []
        for line in lines:
            item = line.strip().strip("'\"")
            item = item[2:].strip().strip("'\"") if item.startswith("- ") else item
            if item in names:
                removed += 1
                continue
            kept.append(line)
        if len(kept) != len(lines):
            workflow.write_text("".join(kept))
    return removed


def regrow_brackets():
    """Brackets were retired on 2026-11-12: nothing routes to them and nothing deploys them, so a
    retirement no longer has to regenerate them. Kept as a no-op so callers stay unchanged."""
    return "brackets retired; nothing to regenerate"


def main(argv):
    names = [arg for arg in argv[1:] if not arg.startswith("--")]
    delete_files = "--delete-files" in argv
    dry = "--dry-run" in argv
    if not names:
        print(__doc__)
        return 2
    if dry:
        print(f"would retire: {', '.join(names)}")
        return 0

    routes = drop_from_gateway(set(names))
    blocks = drop_from_manifests(set(names))
    matrix = drop_from_workflows(set(names))
    print(f"gateway routes removed: {routes}")
    print(f"manifest documents removed: {blocks}")
    print(f"ci matrix entries removed: {matrix}")
    print(regrow_brackets())

    if delete_files:
        for name in names:
            directory = ROOT / name
            if directory.exists():
                subprocess.run(["git", "rm", "-r", "-q", str(directory)], cwd=ROOT)
                print(f"deleted {name}/")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
