"""Regulatory capital, liquidity and leverage ratios.

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

REQUIRED = ("regulatory_ratio_json",)


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


def regulatory_ratios(
    company_id,
    regulator_type,
    tier1_capital,
    tier2_capital,
    risk_weighted_assets,
    total_capital,
    total_assets,
    liquid_assets,
    net_loan,
):
    """Capital, liquidity and leverage ratios against their minimums.

    Every ratio is reported whatever its outcome; the overall verdict is compliant
    only when all seven clear their floor.
    """
    payload = {
        "company_id": company_id,
        "regulator_type": regulator_type,
        "tier1_capital": tier1_capital,
        "tier2_capital": tier2_capital,
        "risk_weighted_assets": risk_weighted_assets,
        "total_capital": total_capital,
        "total_assets": total_assets,
        "liquid_assets": liquid_assets,
        "net_loan": net_loan,
    }
    answer = _call("regulatory_ratio_json", payload)
    if answer is not None:
        return answer

    tier1_ratio = tier1_capital / risk_weighted_assets if risk_weighted_assets else 0
    total_cap_ratio = total_capital / risk_weighted_assets if risk_weighted_assets else 0
    leverage_ratio = tier1_capital / total_assets if total_assets else 0
    liquidity_ratio = liquid_assets / net_loan if net_loan else 0

    def ratio(name, actual, minimum, digits):
        return {
            "ratio_name": name,
            "actual_value": round(actual, digits),
            "minimum_required": minimum,
            "buffer": round(actual - minimum, digits),
            "compliant": actual >= minimum,
        }

    capital = [
        ratio("CET1 Ratio", tier1_ratio, 0.045, 4),
        ratio("Tier 1 Ratio", tier1_ratio, 0.06, 4),
        ratio("Total Capital Ratio", total_cap_ratio, 0.08, 4),
    ]
    liquidity = [ratio("Liquidity Coverage Ratio", liquidity_ratio, 1.0, 2)]
    leverage = [ratio("Leverage Ratio", leverage_ratio, 0.03, 4)]
    all_compliant = all(r["compliant"] for r in capital + liquidity + leverage)
    return {
        "company_id": company_id,
        "regulator_type": regulator_type,
        "capital_ratios": capital,
        "liquidity_ratios": liquidity,
        "leverage_ratios": leverage,
        "overall_compliance": "compliant" if all_compliant else "non_compliant",
        "recommendations": (
            ["Continue monitoring regulatory ratios"]
            if all_compliant
            else [
                "Maintain capital buffers above minimum requirements",
                "Monitor liquidity ratios regularly",
                "Review risk-weighted assets calculation",
            ]
        ),
    }
