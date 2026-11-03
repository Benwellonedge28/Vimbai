"""Tax risk assessment.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("tax_risk_json",)


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


def assess_risk(company_id, assessment_date, risks):
    """Weighted exposure, the overall level, and each risk's own banding.

    The overall level reads the summed weighted exposure, while each risk is banded
    on its own weighted score, so a book of small risks can look safe item by item
    and still be high overall.
    """
    payload = {"company_id": company_id, "assessment_date": assessment_date, "risks": list(risks)}
    answer = _call("tax_risk_json", payload)
    if answer is not None:
        return answer

    total_exposure = sum(r["potential_exposure"] for r in risks)
    weighted = sum(r["potential_exposure"] * r["probability"] for r in risks)
    if weighted > 500000:
        overall = "critical"
    elif weighted > 100000:
        overall = "high"
    elif weighted > 25000:
        overall = "medium"
    else:
        overall = "low"

    detail = []
    for r in risks:
        score = r["potential_exposure"] * r["probability"]
        level = "critical" if score > 250000 else "high" if score > 50000 else "medium" if score > 10000 else "low"
        detail.append(
            {
                "risk_id": r["risk_id"],
                "description": r["description"],
                "risk_type": r["risk_type"],
                "potential_exposure": r["potential_exposure"],
                "probability": r["probability"],
                "weighted_score": round(score, 2),
                "level": level,
                "jurisdiction": r["jurisdiction"],
                "mitigation_actions": r["mitigation_actions"],
            }
        )
    return {
        "company_id": company_id,
        "assessment_date": assessment_date,
        "overall_risk_level": overall,
        "total_exposure": round(total_exposure, 2),
        "weighted_exposure": round(weighted, 2),
        "risk_count": len(risks),
        "risks": detail,
        "recommendations": [
            "Conduct regular transfer pricing documentation reviews",
            "Implement real-time VAT reconciliation to minimize VAT gaps",
            "Monitor permanent establishment risks in cross-border operations",
            "Maintain up-to-date tax treaty position documentation",
            "Engage external tax advisors for high-exposure areas",
        ],
    }
