"""Referee for the depreciation ladder.

Only /method crosses the seam. /calculate does not: its schedule is synthetic - the
same total repeated five times - while the library builds a real ladder, so swapping
engines there would change what clients receive. It stays on its own maths until that
endpoint gets a considered decision rather than a silent one.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("depreciation_method_json",)


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


def run_method(payload, reference):
    """The library's ladder, rebuilt into the response this endpoint publishes.

    The library calls the list `schedule`; this endpoint has always called it
    `depreciation_schedule`, so the wrapper renames rather than the clients.
    """
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    native = None if requested in ("python", "reference") else _extension()
    if native is None:
        if requested == "rust" and _extension() is None:
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is missing or too old")
        result = reference(payload)
        return result if isinstance(result, dict) else result.model_dump()
    answer = json.loads(
        native.depreciation_method_json(
            payload["cost"],
            payload["salvage_value"],
            payload["useful_life"],
            payload["method"],
        )
    )
    return {
        "method": payload["method"],
        "annual_depreciation": answer["annual_depreciation"],
        "monthly_depreciation": answer["monthly_depreciation"],
        "depreciation_schedule": answer["schedule"],
    }
