"""Referee for the credit risk figures.

The figures come from the Rust library when its extension is installed; the Python
maths stays in main.py as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same answer.
"""

import json
import os
import sys

_CALC_DIR = os.path.dirname(os.path.abspath(__file__))


def engine_requested():
    """Which engine the operator asked for: auto, rust or python."""
    return os.environ.get("VIMBAI_CALC_ENGINE", "auto").strip().lower() or "auto"


def _native():
    """The Rust extension, or None when this image was built without it."""
    try:
        import vimbai_native

        return vimbai_native
    except Exception:
        return None


def engine_in_use():
    """The engine that will actually answer: rust or python."""
    wanted = engine_requested()
    if wanted == "python":
        return "python"
    if wanted == "rust":
        return "rust" if _native() is not None else "python"
    return "rust" if _native() is not None else "python"


def _call(function_name, *args):
    """Ask the library, or return None so the caller can fall back."""
    if engine_requested() == "python":
        return None
    native = _native()
    if native is None:
        return None
    function = getattr(native, function_name)
    try:
        answer = json.loads(function(*args))
    except Exception as err:  # a library refusal falls back to the Python maths
        if engine_requested() == "rust":
            raise
        print(f"{function_name} declined: {err}", file=sys.stderr)
        return None
    return answer


def analyze(payload, reference):
    """Credit risk for one borrower: rating, PD, LGD, EAD, expected loss.

    The two derived probabilities are compounded in whichever language answers, and
    Rust's pow lands a digit or so away from Python's. Neither is wrong, so the figure
    is settled at ten places and both engines report the same number.
    """
    answer = _call("credit_risk_json", json.dumps(payload))
    if answer is None:
        answer = reference(payload)
    if hasattr(answer, "model_dump"):
        answer = answer.model_dump()
    pd_calc = answer.get("pd_calculations") or {}
    for key in ("five_year_pd", "lifetime_pd"):
        if isinstance(pd_calc.get(key), float):
            pd_calc[key] = round(pd_calc[key], 10)
    return answer
