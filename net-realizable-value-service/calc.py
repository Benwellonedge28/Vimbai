"""Net realizable value.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("nrv_json",)


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    """The extension, but only when it carries the functions this service needs.

    An older wheel on the image imports cleanly and then fails at the first call,
    so the probe is the functions themselves rather than the module.
    """
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


def _call(name, payload):
    """Ask the extension for one figure set, or None when the reference must answer."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    if requested in ("python", "reference"):
        return None
    native = _extension()
    if native is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is missing or too old")
        return None
    return json.loads(getattr(native, name)(json.dumps(payload)))


def net_realizable_value(selling_price, completion_cost, selling_cost):
    """Selling price less the cost to finish and the cost to sell.

    The formula string is part of the payload, because the service returns it and
    downstream readers quote it; it is built from the unrounded inputs.
    """
    payload = {
        "selling_price": selling_price,
        "completion_cost": completion_cost,
        "selling_cost": selling_cost,
    }
    answer = _call("nrv_json", payload)
    if answer is not None:
        return answer

    nrv = selling_price - completion_cost - selling_cost
    return {
        "selling_price": selling_price,
        "completion_cost": completion_cost,
        "selling_cost": selling_cost,
        "net_realizable_value": nrv,
        "formula": f"{selling_price} - {completion_cost} - {selling_cost} = {nrv}",
    }
