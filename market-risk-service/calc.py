"""Portfolio value at risk.

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

REQUIRED = ("var_json",)


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


def value_at_risk(company_id, portfolio_name, positions, confidence_level, holding_period_days):
    """Portfolio VaR at 95% and 99%, expected shortfall, and two stress losses.

    The diversification benefit is still the flat 15% haircut the service always
    applied rather than a correlation matrix, so figures stay comparable month to
    month until that is deliberately replaced.
    """
    payload = {
        "company_id": company_id,
        "portfolio_name": portfolio_name,
        "positions": list(positions),
        "confidence_level": confidence_level,
        "holding_period_days": holding_period_days,
    }
    answer = _call("var_json", payload)
    if answer is not None:
        return answer

    total = sum(p["exposure"] for p in positions)
    weighted_vol = sum(p["exposure"] * p["volatility"] for p in positions) / total if total else 0
    portfolio_vol = weighted_vol * 0.85
    span = math.sqrt(holding_period_days)
    var_95 = total * portfolio_vol * 1.645 * span
    var_99 = total * portfolio_vol * 2.326 * span
    if var_95 / total > 0.20:
        risk = "high"
    elif var_95 / total > 0.10:
        risk = "medium"
    else:
        risk = "low"
    return {
        "company_id": company_id,
        "portfolio_name": portfolio_name,
        "total_exposure": round(total, 2),
        "portfolio_volatility": round(portfolio_vol, 4),
        "var_95": round(var_95, 2),
        "var_99": round(var_99, 2),
        "expected_shortfall": round(total * portfolio_vol * 2.063 * span, 2),
        "stress_loss_2sd": round(total * portfolio_vol * 2, 2),
        "stress_loss_3sd": round(total * portfolio_vol * 3, 2),
        "risk_level": risk,
    }
