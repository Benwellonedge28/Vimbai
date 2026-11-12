"""The migration backlog is derived, so it cannot silently drift from the codebase."""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

import migration_backlog  # noqa: E402


def test_every_routed_service_is_classified_once():
    rows = migration_backlog.survey()
    names = [r["name"] for r in rows]
    assert len(names) == len(set(names)), "a service appears twice in the backlog survey"
    assert rows, "no services surveyed - did the gateway registry move?"


def test_a_port_needs_named_evidence():
    port_map = (ROOT / "rust" / "CALCULATOR-PORT.md").read_text()
    for row in migration_backlog.survey():
        if row["rust_engine"] == "none":
            assert not re.search(r"\b" + re.escape(row["name"]) + r"\b", port_map), (
                "%s is described as ported in CALCULATOR-PORT.md but is not counted as ported" % row["name"]
            )


def test_the_document_is_regenerated():
    rows = migration_backlog.survey()
    text, routed, ported, pending = migration_backlog.render(rows)
    doc = (ROOT / "docs" / "RUST-MIGRATION-BACKLOG.md").read_text()
    assert doc == text, "run python3 scripts/migration_backlog.py - the backlog doc is stale"
