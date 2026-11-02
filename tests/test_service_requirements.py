"""Every service that imports neo4j must declare it in its requirements.txt.

A service whose Python imports neo4j at module scope but does not pin it fails
in two places: the CI job's collection step (ModuleNotFoundError before a single
test runs) and the built image, which installs only its own requirements.txt.
This sweep keeps both honest.
"""

import pathlib
import re

import pytest

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
NEO4J_IMPORT = re.compile(r"^\s*(?:from|import)\s+neo4j\b", re.MULTILINE)


def _service_dirs():
    return sorted(
        p
        for p in REPO_ROOT.iterdir()
        if p.is_dir() and (p / "requirements.txt").is_file() and (p / "main.py").is_file()
    )


def _imports_neo4j(service_dir):
    for path in service_dir.rglob("*.py"):
        if "__pycache__" in path.parts or "tests" in path.parts:
            continue
        if NEO4J_IMPORT.search(path.read_text(errors="ignore")):
            return True
    return False


@pytest.mark.parametrize("service_dir", _service_dirs(), ids=lambda p: p.name)
def test_service_pins_neo4j_in_its_own_requirements(service_dir):
    if not _imports_neo4j(service_dir):
        pytest.skip("does not import neo4j")
    requirements = (service_dir / "requirements.txt").read_text().lower()
    declared = [line.strip() for line in requirements.splitlines() if line.strip() and not line.strip().startswith("#")]
    assert any(
        re.match(r"^neo4j(\[[^\]]*\])?([<>=!~]=?.*)?$", line) for line in declared
    ), f"{service_dir.name} imports neo4j but its requirements.txt does not declare it"


@pytest.mark.parametrize("service_dir", _service_dirs(), ids=lambda p: p.name)
def test_requirements_declare_no_duplicates(service_dir):
    seen = {}
    for line in (service_dir / "requirements.txt").read_text().splitlines():
        entry = line.strip()
        if not entry or entry.startswith("#"):
            continue
        name = re.split(r"[<>=!~\[; ]", entry, maxsplit=1)[0].lower()
        assert name not in seen, f"{service_dir.name} declares {name} twice"
        seen[name] = entry
