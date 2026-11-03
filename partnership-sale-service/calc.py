"""Partner retirement and goodwill.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("partner_buyout_json", "partnership_goodwill_json")


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


def retirement(
    company_id,
    outgoing_partner_name,
    capital_balance,
    share_of_goodwill,
    share_of_reserves,
    loan_to_partner,
    agreed_payment,
    remaining_partners,
):
    """What the outgoing partner is owed, and how the difference lands on the rest.

    A payment above the amount due is a loss on retirement, and the difference is
    spread over the remaining partners by their share, not by their capital.
    """
    payload = {
        "company_id": company_id,
        "outgoing_partner_name": outgoing_partner_name,
        "capital_balance": capital_balance,
        "share_of_goodwill": share_of_goodwill,
        "share_of_reserves": share_of_reserves,
        "loan_to_partner": loan_to_partner,
        "agreed_payment": agreed_payment,
        "remaining_partners": list(remaining_partners),
    }
    answer = _call("partner_buyout_json", payload)
    if answer is not None:
        return answer

    total_payable = capital_balance + share_of_goodwill + share_of_reserves + loan_to_partner
    difference = total_payable - agreed_payment

    adjustments = []
    if remaining_partners:
        total_share = sum(float(p.get("share_pct", 0)) for p in remaining_partners)
        for p in remaining_partners:
            share = float(p.get("share_pct", 0)) / total_share if total_share else 0
            adjustments.append(
                {
                    "partner_id": p.get("partner_id", ""),
                    "name": p.get("name", ""),
                    "capital_adjustment": round(difference * share, 2),
                    "share_pct": p.get("share_pct", 0),
                }
            )
    return {
        "company_id": company_id,
        "outgoing_partner": outgoing_partner_name,
        "capital_due": round(capital_balance, 2),
        "goodwill_due": round(share_of_goodwill, 2),
        "reserves_due": round(share_of_reserves, 2),
        "loan_due": round(loan_to_partner, 2),
        "total_payable": round(total_payable, 2),
        "agreed_payment": round(agreed_payment, 2),
        "difference": round(difference, 2),
        "gain_or_loss_on_retirement": round(agreed_payment - total_payable, 2),
        "remaining_partner_adjustments": adjustments,
    }


def goodwill(company_id, partners, avg_profit, years, rate):
    """Super profit over the normal return on capital, capitalised for a number of years.

    Each partner's slice comes from their share percentage, so the slices only add
    up to the whole when the shares do.
    """
    payload = {
        "company_id": company_id,
        "partners": list(partners),
        "avg_profit": avg_profit,
        "years": years,
        "rate": rate,
    }
    answer = _call("partnership_goodwill_json", payload)
    if answer is not None:
        return answer

    total_capital = sum(float(p.get("capital", 0)) for p in partners)
    super_profit = avg_profit - (total_capital * rate)
    goodwill_total = super_profit * years
    return {
        "company_id": company_id,
        "total_goodwill": round(goodwill_total, 2),
        "super_profit": round(super_profit, 2),
        "normal_return_rate": rate,
        "partner_goodwill_shares": {
            p.get("partner_id", ""): round(goodwill_total * (float(p.get("share_pct", 0)) / 100), 2) for p in partners
        },
    }
