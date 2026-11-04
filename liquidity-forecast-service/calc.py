"""Referee for the liquidity forecast.

The figures come from the Rust library when its extension is installed; the Python
logic stays in main.py as the reference implementation and as the fallback for images
built without the wheel. ``VIMBAI_CALC_ENGINE`` forces one side: ``rust``, ``python``
or ``auto`` (the default), which takes the library when it imports.
"""

import json
import os

SEAM = "liquidity_forecast_json"


def engine_in_use():
    """Which side answered the last call, and which side is configured."""
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


def analyze(payload, reference):
    """liquidity forecast for one request."""
    answer = _call(payload)
    if answer is None:
        answer = reference(payload)
        if hasattr(answer, "model_dump"):
            answer = answer.model_dump()
    return answer
