import importlib
import importlib.util
import os
import sys
from types import ModuleType

# Add project root to path
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, root)

_service_cache = {}


def _load_service(service_dir_name, root):
    if service_dir_name in _service_cache:
        return _service_cache[service_dir_name]

    service_path = os.path.join(root, service_dir_name, "main.py")
    if not os.path.exists(service_path):
        return None

    module_name = service_dir_name.replace("-", "_").replace(".", "_")

    spec = importlib.util.spec_from_file_location(f"{module_name}.main", service_path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[f"{module_name}.main"] = module

    try:
        spec.loader.exec_module(module)
    except ImportError:
        # Service has missing deps - skip it, return None
        return None
    except Exception:
        return None

    pkg = ModuleType(module_name)
    pkg.main = module
    sys.modules[module_name] = pkg

    _service_cache[service_dir_name] = pkg
    return pkg


# Only load services we want to test on demand, not all at once
def pytest_configure(config):
    """Load test services lazily via fixtures"""
    pass


# Services that have been deleted once their arithmetic moved into the Rust library. Tests
# that exercised them, or compared them against the seam, are obsolete by definition: there
# is no Python left to run. They skip with this reason rather than erroring, so the suite says
# plainly why a case is not running.
def service_missing(name):
    return not os.path.isdir(os.path.join(root, name))


# Make loader available to test files
def load_service(name):
    if service_missing(name):
        import pytest

        pytest.skip(f"{name} has been deleted; its maths lives in the Rust library")
    return _load_service(name, root)
