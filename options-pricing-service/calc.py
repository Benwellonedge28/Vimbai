"""Options pricing: Black-Scholes values and the greeks.

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

from scipy.stats import norm

REQUIRED = ("options_price_json",)


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


def price(company_id, options, current_spot):
    """Value a book of European options and total up the exposure.

    Theta is reported per day, not per year, and vega per percentage point of
    volatility, which is why both are divided down before they are summed. The
    greeks are aggregated weighted by notional, so the summary is the sensitivity of
    the whole book rather than of one contract.
    """
    payload = {"company_id": company_id, "options": list(options), "current_spot": current_spot}
    answer = _call("options_price_json", payload)
    if answer is None:
        answer = _reference(payload)
    # The premium is quoted to four places, so the residual time value is quoted to
    # four as well. Left raw it carries the last bits of whichever normal CDF did the
    # work, and those differ between the library and scipy at the fifteenth digit.
    for valuation in answer["option_valuations"]:
        valuation["time_value"] = round(valuation["time_value"], 4)
    return answer


def _reference(payload):
    """The original Python pricing, kept as the oracle."""
    valuations = []
    total_premium = total_delta = total_gamma = total_theta = total_vega = 0.0

    for opt in payload["options"]:
        s, k = opt["spot_price"], opt["strike_price"]
        t, r, sigma = opt["time_to_expiry"], opt["risk_free_rate"], opt["volatility"]
        is_call = opt["option_type"] == "call"

        d1 = (math.log(s / k) + (r + sigma**2 / 2) * t) / (sigma * math.sqrt(t))
        d2 = d1 - sigma * math.sqrt(t)
        if is_call:
            value = s * norm.cdf(d1) - k * math.exp(-r * t) * norm.cdf(d2)
            intrinsic = max(s - k, 0.0)
        else:
            value = k * math.exp(-r * t) * norm.cdf(-d2) - s * norm.cdf(-d1)
            intrinsic = max(k - s, 0.0)

        delta = norm.cdf(d1) if is_call else norm.cdf(d1) - 1
        gamma = norm.pdf(d1) / (s * sigma * math.sqrt(t))
        theta = (
            -s * norm.pdf(d1) * sigma / (2 * math.sqrt(t)) - r * k * math.exp(-r * t) * norm.cdf(d1 if is_call else -d1)
        ) / 365
        vega = s * math.sqrt(t) * norm.pdf(d1) / 100

        if is_call:
            moneyness = "ITM" if s > k else "ATM" if s == k else "OTM"
        else:
            moneyness = "ITM" if s < k else "ATM" if s == k else "OTM"

        valuations.append(
            {
                "type": opt["option_type"],
                "strike": k,
                "spot": s,
                "premium": round(value, 4),
                "premium_pct": round(value / s * 100, 4),
                "moneyness": moneyness,
                "delta": round(delta, 4),
                "gamma": round(gamma, 6),
                "theta": round(theta, 4),
                "vega": round(vega, 4),
                "intrinsic_value": intrinsic,
                "time_value": value - intrinsic,
            }
        )

        total_premium += value * opt["notional"]
        total_delta += delta * opt["notional"]
        total_gamma += gamma * opt["notional"]
        total_theta += theta * opt["notional"]
        total_vega += vega * opt["notional"]

    notional_spot = sum(o["spot_price"] * o["notional"] for o in payload["options"])
    recommendations = []
    if abs(total_delta) > sum(o["notional"] for o in payload["options"]) * 0.8:
        recommendations.append("High delta exposure - consider delta hedging")
    if total_gamma > 0.1:
        recommendations.append("Gamma risk elevated - monitor position closely")
    if round(total_theta, 4) < -10000:
        recommendations.append("Significant time decay - assess holding period")

    return {
        "company_id": payload["company_id"],
        "option_valuations": valuations,
        "portfolio_summary": {
            "total_premium": round(total_premium, 2),
            "option_count": len(payload["options"]),
            "avg_premium_pct": round(total_premium / notional_spot * 100, 4) if payload["options"] else 0,
        },
        "greeks_summary": {
            "delta": round(total_delta, 4),
            "gamma": round(total_gamma, 6),
            "theta": round(total_theta, 4),
            "vega": round(total_vega, 4),
        },
        "recommendations": recommendations,
    }
