"""Partnership profit allocation.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("partnership_allocation_json",)


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    """The extension, but only when it carries the functions this service needs.

    An older wheel on the image imports cleanly and then fails at the first call,
    so the probe is the functions themselves rather than the module.
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


def allocate(company_id, period, partners, net_profit, interest_rate_on_capital):
    """Salaries and capital interest first, then the residue by profit share.

    Where every partner carries a zero share the split falls back to equal, and a
    loss after salaries and interest is treated as no residue at all rather than a
    negative one to divide.
    """
    payload = {
        "company_id": company_id,
        "period": period,
        "partners": list(partners),
        "net_profit": net_profit,
        "interest_rate_on_capital": interest_rate_on_capital,
    }
    answer = _call("partnership_allocation_json", payload)
    if answer is not None:
        return answer

    partners = [dict(p) for p in partners]
    total_salary = sum(p["salary"] for p in partners)
    total_interest = sum(p["capital_contribution"] * interest_rate_on_capital for p in partners)
    residue = max(net_profit - total_salary - total_interest, 0)

    if sum(p["profit_share_pct"] for p in partners) == 0:
        equal = 100 / len(partners)
        for p in partners:
            p["profit_share_pct"] = equal

    accounts = []
    total_share = 0.0
    for p in partners:
        interest = p["capital_contribution"] * interest_rate_on_capital
        share = residue * (p["profit_share_pct"] / 100)
        total_share += share
        closing = p["capital_contribution"] + p["salary"] + interest + share - p["drawings"]
        accounts.append(
            {
                "partner_id": p["partner_id"],
                "name": p["name"],
                "opening_capital": p["capital_contribution"],
                "salary": p["salary"],
                "interest_on_capital": round(interest, 2),
                "profit_share": round(share, 2),
                "drawings": p["drawings"],
                "closing_capital": round(closing, 2),
            }
        )
    return {
        "company_id": company_id,
        "period": period,
        "total_salary": round(total_salary, 2),
        "total_interest": round(total_interest, 2),
        "total_profit_share": round(total_share, 2),
        "residual_profit": round(residue, 2),
        "partner_accounts": accounts,
    }
