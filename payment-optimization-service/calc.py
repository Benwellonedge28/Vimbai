"""Referee for the payment optimisation.

The decisions come from the Rust library when its extension is installed; the Python
logic stays in main.py as the reference implementation and as the fallback for images
built without the wheel. ``VIMBAI_CALC_ENGINE`` forces one side.

The library keeps every amount as a decimal; the seam converts to the floats this
service already publishes, so the loss is at the edge and nowhere earlier.
"""

import json
import os

SEAM = "payment_optimization_json"


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


def optimize(payload, reference):
    answer = _call(payload)
    if answer is None:
        answer = reference(payload)
        if hasattr(answer, "model_dump"):
            answer = answer.model_dump()
    return answer
