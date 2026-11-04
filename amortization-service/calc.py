"""Referee for the amortization figures.

The figures come from the Rust library when its extension is installed; the Python
maths stays in main.py as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds both to the same answers.
"""

import importlib.util
import json
import os

_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("amortization_vimbai_calc", os.path.join(_CALC_DIR, "calc.py"))
CALC_PATH = _spec.origin if _spec and _spec.origin else __file__

_ENGINE = os.environ.get("VIMBAI_CALC_ENGINE", "auto").strip().lower() or "auto"
_native = None
_native_checked = False


def _library():
    """The extension, or None when this image was built without it."""
    global _native, _native_checked
    if not _native_checked:
        _native_checked = True
        try:
            import vimbai_native

            _native = vimbai_native
        except Exception:
            _native = None
    return _native


def engine_in_use():
    if _ENGINE == "python":
        return "python"
    if _ENGINE == "rust":
        return "rust" if _library() else "python"
    return "rust" if _library() else "python"


def _call(function, *args):
    """Returns the library's answer, or None to fall through to the reference."""
    if _ENGINE == "python":
        return None
    library = _library()
    if library is None or not hasattr(library, function):
        return None
    return json.loads(getattr(library, function)(*args))


def calculate(company_id, intangibles, period_start, period_end, reference):
    answer = _call("amortization_json", json.dumps(list(intangibles)))
    if answer is None:
        answer = reference(intangibles, period_start, period_end)
    return {
        "company_id": company_id,
        "period": {"start": str(period_start), "end": str(period_end)},
        "total_amortization": answer["total_amortization"],
        "total_net_book_value": answer["total_net_book_value"],
        "schedules": answer["schedules"],
    }
