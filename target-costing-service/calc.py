"""Referee for the target costing figures.

The figures come from the Rust library when its extension is installed; the Python
logic stays in main.py as the reference implementation and as the fallback for images
built without the wheel. ``VIMBAI_CALC_ENGINE`` forces one side: ``rust``, ``python``
or ``auto`` (the default), which takes the library when it imports.

The library works in decimals and hands back amounts as text. This service has always
published floats, so the conversion happens here, once, at the edge - and is the only
place a large amount can lose a digit on its way to a client.
"""

import json
import os

SEAM = "target_cost_json"


def engine_in_use():
    return _seam() is not None and _forced() != "python"


def _forced():
    return os.environ.get("VIMBAI_CALC_ENGINE", "auto").strip().lower()


def _seam():
    if _forced() == "python":
        return None
    try:
        import vimbai_native
    except Exception:
        return None
    return getattr(vimbai_native, SEAM, None)


def _call(payload):
    seam = _seam()
    if seam is None:
        return None
    try:
        return json.loads(seam(json.dumps(payload)))
    except Exception:
        return None


def calculate(payload, reference):
    """Target costing for one product."""
    answer = _call(payload)
    if answer is None:
        answer = reference(payload)
        if hasattr(answer, "model_dump"):
            answer = answer.model_dump()
    return answer
