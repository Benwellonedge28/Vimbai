"""Referee for the corporate tax figures.

The figures come from the Rust library when its extension is installed; the Python
maths stays in main.py as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("corporate_tax_json",)


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    """The extension, but only when it carries the functions this service needs."""
    try:
        import vimbai_native
    except ImportError:
        return None
    if not all(hasattr(vimbai_native, name) for name in REQUIRED):
        return None
    return vimbai_native


def engine_in_use() -> str:
    """Announce the calculator so /health can be read in production."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    if requested in ("python", "reference"):
        return "python"
    if _extension() is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is missing or too old")
        return "python"
    return "rust"


def run(payload, reference):
    """The library's answer, or the reference when the wheel is not installed."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    native = None if requested in ("python", "reference") else _extension()
    if native is None:
        if requested == "rust" and _extension() is None:
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is missing or too old")
        result = reference(payload)
        return result if isinstance(result, dict) else result.model_dump()
    return json.loads(native.corporate_tax_json(json.dumps(payload)))
