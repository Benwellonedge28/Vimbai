"""R&D credit calculations.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


REQUIRED = ("tax_credit_json",)


def _extension():
    """The extension, but only when it carries the function this service needs.

    An older wheel on the image imports cleanly and then fails at the first call,
    so the probe is the function itself rather than the module.
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


DEFAULT_CREDIT_RATE = 0.15
DEFAULT_ALTERNATIVE_RATE = 0.20


def credit_claim(company_id, fiscal_year, expenditures, credit_rate, alternative_rate):
    """Qualifying spend, both credit methods, and the recommendation.

    The two methods are compared as calculated rather than as rounded, so a claim
    that differs only in the second decimal still recommends the larger one.
    """
    payload = {
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "credit_rate": credit_rate,
        "alternative_rate": alternative_rate,
        "expenditures": list(expenditures),
    }
    answer = _call("tax_credit_json", payload)
    if answer is not None:
        return answer

    total = sum(e["amount"] for e in expenditures)
    qualifying = sum(e["amount"] for e in expenditures if e["qualifies"])
    credit_reg = qualifying * credit_rate
    credit_alt = qualifying * alternative_rate
    breakdown = {}
    for e in expenditures:
        if e["qualifies"]:
            breakdown[e["category"]] = breakdown.get(e["category"], 0) + e["amount"]
    return {
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "total_expenditure": round(total, 2),
        "qualifying_expenditure": round(qualifying, 2),
        "non_qualifying": round(total - qualifying, 2),
        "credit_regular": round(credit_reg, 2),
        "credit_alternative": round(credit_alt, 2),
        "recommended_method": "regular" if credit_reg > credit_alt else "alternative_simplified",
        "expenditure_breakdown": {k: round(v, 2) for k, v in breakdown.items()},
    }
