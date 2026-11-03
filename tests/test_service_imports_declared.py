"""A service must declare every third-party module it imports.

npo-service imported `load_dotenv` while its requirements pinned `dotenv`
(an unrelated package), so CI failed to import the module even though the
whole suite passed locally where python-dotenv happened to be installed.
56 more services relied on `structlog` arriving transitively from another
package - it worked, but nothing guaranteed it.

This test keeps the import surface and the install surface in step, using
only the stdlib so it runs without installing anything.
"""

import ast
import json
import sys
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[1]
GATEWAY_CONFIG = REPO / "api-gateway" / "config" / "services.json"

# import name -> distribution name, where the two differ
DIST_NAME = {
    "dotenv": "python-dotenv",
    "jose": "python-jose",
    "yaml": "pyyaml",
    "PIL": "pillow",
    "sklearn": "scikit-learn",
    "multipart": "python-multipart",
    "dateutil": "python-dateutil",
    "jwt": "pyjwt",
}

STDLIB = set(sys.stdlib_module_names) | set(sys.builtin_module_names)


def _services():
    return [s["name"] for s in json.loads(GATEWAY_CONFIG.read_text())["services"]]


def _imported_modules(path: Path):
    tree = ast.parse(path.read_text(encoding="utf-8", errors="replace"))
    found = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            found |= {alias.name.split(".")[0] for alias in node.names}
        elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
            found.add(node.module.split(".")[0])
    return found


def _is_local(service: str, module: str) -> bool:
    dir_path = REPO / service
    if (dir_path / f"{module}.py").is_file() or (dir_path / module).is_dir():
        return True
    # the service's own package module (dir name with underscores)
    return module in {service.replace("-", "_"), f"{service.replace('-', '_')}_service", "shared"}


@pytest.mark.parametrize("service", _services())
def test_imports_are_declared_in_requirements(service):
    main = REPO / service / "main.py"
    requirements = REPO / service / "requirements.txt"
    if not main.is_file() or not requirements.is_file():
        pytest.skip(f"{service} keeps no requirements.txt")
    declared = requirements.read_text(encoding="utf-8", errors="replace").lower().replace("_", "-")

    undeclared = []
    for module in sorted(_imported_modules(main)):
        if module in STDLIB or module.startswith("_") or _is_local(service, module):
            continue
        distribution = DIST_NAME.get(module, module).lower().replace("_", "-")
        if distribution not in declared:
            undeclared.append(module)

    assert not undeclared, (
        f"{service}: imports {undeclared} but requirements.txt never installs them; "
        f"the image build would fail on import"
    )
