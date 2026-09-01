import sys
import os
import importlib
import importlib.util
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

# Make loader available to test files
def load_service(name):
    return _load_service(name, root)
