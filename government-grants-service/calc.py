"""Government grant recognition.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("grant_json",)


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


def recognize_grant(
    company_id,
    grant_name,
    granting_authority,
    grant_amount,
    grant_type,
    recognition_method,
    asset_useful_life,
    related_asset_cost,
    useful_life_years,
    conditions,
    compliance_status,
):
    """Annual recognition, the deferred balance, and the narrative entries.

    An asset grant is spread over the asset's life; every other grant is income and
    is recognised in full on receipt, with nothing left deferred.
    """
    payload = {
        "company_id": company_id,
        "grant_name": grant_name,
        "granting_authority": granting_authority,
        "grant_amount": grant_amount,
        "grant_type": grant_type,
        "recognition_method": recognition_method,
        "asset_useful_life": asset_useful_life,
        "related_asset_cost": related_asset_cost,
        "useful_life_years": useful_life_years,
        "conditions": list(conditions),
        "compliance_status": compliance_status,
    }
    answer = _call("grant_json", payload)
    if answer is not None:
        return answer

    if grant_type == "asset":
        annual = grant_amount / asset_useful_life if asset_useful_life else 0
        deferred = grant_amount - annual
    else:
        annual = grant_amount
        deferred = 0
    return {
        "company_id": company_id,
        "grant_name": grant_name,
        "granting_authority": granting_authority,
        "grant_amount": round(grant_amount, 2),
        "grant_type": grant_type,
        "recognition_method": recognition_method,
        "annual_recognition": round(annual, 2),
        "annual_amortization": round(annual, 2),
        "deferred_income_balance": round(deferred, 2),
        "journal_entries": [
            f"Dr Bank {round(grant_amount, 2)}",
            f"Cr Deferred Income {round(grant_amount, 2)}",
        ],
        "disclosure_notes": [
            f"Grant received from {granting_authority or 'government authority'}",
            f"Recognized over {asset_useful_life} years",
            f"Conditions: {', '.join(conditions) if conditions else 'None specified'}",
        ],
        "conditions": list(conditions),
        "compliance_status": compliance_status,
    }
