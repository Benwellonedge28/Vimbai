"""Fraud risk assessment: the fraud triangle, scored.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("fraud_risk_json",)


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


def _indicator(category, indicator, risk_level, weight):
    return {"category": category, "indicator": indicator, "risk_level": risk_level, "weight": weight}


def assess(
    audit_id,
    company_id,
    industry,
    company_size,
    financial_indicators,
    management_characteristics,
    internal_control_gaps,
    prior_fraud_history,
):
    """Score the three sides of the fraud triangle and say what to look at.

    Pressure and opportunity carry 40% each and rationalisation 20%, so a manager
    under pressure with no way to act on it scores lower than the same pressure with
    the controls that let it happen. The weights are the ones the endpoint has always
    used; changing them changes every published assessment.
    """
    payload = {
        "audit_id": audit_id,
        "company_id": company_id,
        "industry": industry,
        "company_size": company_size,
        "financial_indicators": financial_indicators,
        "management_characteristics": list(management_characteristics),
        "internal_control_gaps": list(internal_control_gaps),
        "prior_fraud_history": bool(prior_fraud_history),
    }
    answer = _call("fraud_risk_json", payload)
    if answer is not None:
        return answer
    return _reference(payload)


def _reference(payload):
    """The original Python assessment, kept as the oracle."""
    pressure, opportunity, rationalization = [], [], []
    pressure_score = opportunity_score = rationalization_score = 0.0

    if payload["prior_fraud_history"]:
        pressure.append(_indicator("pressure", "Prior fraud incidents", "high", 0.3))
        pressure_score += 0.3
    if payload["company_size"] == "small":
        pressure.append(_indicator("pressure", "Limited resources", "medium", 0.2))
        pressure_score += 0.2
    if "high_turnover" in payload["management_characteristics"]:
        pressure.append(_indicator("pressure", "Management turnover", "high", 0.25))
        pressure_score += 0.25

    gaps = payload["internal_control_gaps"]
    if len(gaps) > 3:
        opportunity.append(_indicator("opportunity", "Multiple control deficiencies", "high", 0.35))
        opportunity_score += 0.35
    if "segregation_of_duties" in gaps:
        opportunity.append(_indicator("opportunity", "Inadequate segregation", "high", 0.3))
        opportunity_score += 0.3

    if "aggressive_targets" in payload["management_characteristics"]:
        rationalization.append(_indicator("rationalization", "Aggressive targets", "medium", 0.2))
        rationalization_score += 0.2

    total = (pressure_score * 0.4 + opportunity_score * 0.4 + rationalization_score * 0.2) * 100
    category = "high" if total >= 70 else "medium" if total >= 40 else "low"

    areas = []
    if opportunity_score > 0.3:
        areas.extend(["Revenue recognition", "Management override"])
    if pressure_score > 0.3:
        areas.append("Financial position manipulation")
    if rationalization_score > 0.2:
        areas.append("Expense capitalization")

    # An empty side of the triangle is reported as a zero-weight place rather than an
    # empty list, which is what this endpoint has always returned.
    return {
        "audit_id": payload["audit_id"],
        "fraud_risk_score": round(total, 2),
        "risk_category": category,
        "pressure_indicators": pressure or [_indicator("pressure", "No significant pressures identified", "low", 0.0)],
        "opportunity_indicators": opportunity or [_indicator("opportunity", "Controls appear adequate", "low", 0.0)],
        "rationalization_indicators": rationalization
        or [_indicator("rationalization", "No concerning rationalization", "low", 0.0)],
        "high_risk_areas": areas or ["No high risk areas identified"],
        "recommended_procedures": (
            [
                "Enhanced management inquiry",
                "Journal entry testing",
                "Significant estimates review",
                "Related party transaction testing",
            ]
            if category == "high"
            else ["Standard substantive procedures"]
        ),
        "conclusion": f"Fraud risk assessed as {category} with score of {round(total, 2)}",
    }
