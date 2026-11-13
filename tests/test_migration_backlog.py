"""The migration backlog is derived, so it cannot silently drift from the codebase."""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

import migration_backlog  # noqa: E402
import migration_manifest  # noqa: E402


def test_every_routed_service_is_classified_once():
    rows = migration_backlog.survey()
    names = [r["name"] for r in rows]
    assert len(names) == len(set(names)), "a service appears twice in the backlog survey"
    assert rows, "no services surveyed - did the gateway registry move?"


def _wired_section():
    """The services the tracker lists as served from Rust, and nothing else.

    This used to be a second copy of the tracker reader that only took the first name on a bullet,
    so a crate that answers for several services reported one ported and the rest unported. The
    manifest already reads that list - one reader, one answer.
    """
    return migration_manifest.ported_names()


def test_a_port_needs_named_evidence():
    """Both directions: wired means ported, and unwired means not ported.

    The second half is the one that was missing. The tracker names roughly a hundred services it is
    still waiting on - "the crate has the maths, the service is not wired", "needs the maths written
    in Rust" - and matching a name anywhere in the file counted all of them as ported, along with
    `currency-service` (inside `multi-currency-service`) and `risk-assessment-service` (inside
    `fraud-risk-assessment-service`). The estate was reported as far more Rust than it is.
    """
    wired = _wired_section()
    for row in migration_backlog.survey():
        name = row["name"]
        if row["rust_engine"] == "rust http":
            assert name in wired, "%s is served from Rust but not listed as wired" % name
        elif row["rust_engine"] == "none":
            assert name not in wired, "%s is wired to the Rust library but counted as unported" % name


def test_the_document_is_regenerated():
    rows = migration_backlog.survey()
    text, routed, ported, pending = migration_backlog.render(rows)
    doc = (ROOT / "docs" / "RUST-MIGRATION-BACKLOG.md").read_text()
    assert doc == text, "run python3 scripts/migration_backlog.py - the backlog doc is stale"
