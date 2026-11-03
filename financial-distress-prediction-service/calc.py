"""Altman Z-score distress prediction.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import math
import os

REQUIRED = ("distress_json",)


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


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


def predict_distress(company):
    """Altman Z-score, the zone it falls in, and the plain-language reading.

    The manufacturing thresholds differ from every other industry, exactly as the
    service always treated them.
    """
    payload = {"company": company}
    answer = _call("distress_json", payload)
    if answer is not None:
        return answer

    z1 = (company["working_capital"] / company["total_assets"]) * 1.2
    z2 = (company["retained_earnings"] / company["total_assets"]) * 1.4
    z3 = (company["ebit"] / company["total_assets"]) * 3.3
    z4 = (company["total_equity"] / company["total_liabilities"]) * 0.6
    z5 = (company["revenue"] / company["total_assets"]) * 1.0
    z_score = z1 + z2 + z3 + z4 + z5

    if company["industry"] == "manufacturing":
        safe, grey, safe_p, grey_p, distress_p = 2.99, 1.81, 0.03, 0.35, 0.95
    else:
        safe, grey, safe_p, grey_p, distress_p = 2.6, 1.5, 0.05, 0.40, 0.90
    if z_score > safe:
        zone, probability = "SAFE", safe_p
    elif z_score > grey:
        zone, probability = "GREY", grey_p
    else:
        zone, probability = "DISTRESS", distress_p

    if z_score > 3.0:
        years, recommendation = 5.0, (
            "Company is financially healthy. Continue monitoring and maintain current strategies."
        )
    elif z_score > 2.0:
        years, recommendation = 2.5, (
            "Company shows signs of stress. Consider reviewing operations and improving liquidity."
        )
    elif z_score > 1.0:
        years, recommendation = 1.0, ("High risk of distress. Urgent action needed - consider restructuring options.")
    else:
        years, recommendation = 0.5, ("Near-term bankruptcy risk. Seek professional financial advice immediately.")
    return {
        "company_id": company["company_id"],
        "z_score": round(z_score, 2),
        "z1": round(z1, 2),
        "z2": round(z2, 2),
        "z3": round(z3, 2),
        "z4": round(z4, 2),
        "z5": round(z5, 2),
        "zone": zone,
        "probability_distress": probability,
        "years_to_distress": years,
        "recommendation": recommendation,
    }
