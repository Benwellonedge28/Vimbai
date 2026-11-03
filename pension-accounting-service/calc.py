"""Defined benefit pension accounting.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("pension_json",)


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


def pension_expense(
    company_id,
    fiscal_year,
    plan_name,
    obligation_beginning,
    service_cost,
    interest_cost,
    benefits_paid,
    actuarial_gain_loss,
    past_service_cost,
    plan_assets_beginning,
    contributions,
    expected_return,
    actual_return,
    benefits_paid_from_assets,
):
    """Close the defined benefit obligation and the plan assets, then read the expense.

    Net interest is interest cost less the expected return on assets, so a plan whose
    assets earn their expected return carries no return surprise; remeasurements take
    the actuarial movement plus any gap between actual and expected return.
    """
    payload = {
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "plan_name": plan_name,
        "obligation_beginning": obligation_beginning,
        "service_cost": service_cost,
        "interest_cost": interest_cost,
        "benefits_paid": benefits_paid,
        "actuarial_gain_loss": actuarial_gain_loss,
        "past_service_cost": past_service_cost,
        "plan_assets_beginning": plan_assets_beginning,
        "contributions": contributions,
        "expected_return": expected_return,
        "actual_return": actual_return,
        "benefits_paid_from_assets": benefits_paid_from_assets,
    }
    answer = _call("pension_json", payload)
    if answer is not None:
        return answer

    dbo_end = (
        obligation_beginning + service_cost + interest_cost + actuarial_gain_loss + past_service_cost - benefits_paid
    )
    actual = actual_return or expected_return
    assets_end = plan_assets_beginning + contributions + actual - benefits_paid_from_assets
    funded = assets_end - dbo_end
    net_liability = max(-funded, 0)
    net_asset = max(funded, 0)
    net_interest = interest_cost - expected_return
    return {
        "company_id": company_id,
        "plan_name": plan_name,
        "fiscal_year": fiscal_year,
        "dbo_end": round(dbo_end, 2),
        "plan_assets_end": round(assets_end, 2),
        "funded_position": round(funded, 2),
        "net_defined_benefit_liability": round(net_liability, 2),
        "current_service_cost": round(service_cost, 2),
        "interest_cost": round(interest_cost, 2),
        "net_interest": round(net_interest, 2),
        "remeasurements": round(actuarial_gain_loss + (actual - expected_return), 2),
        "total_pension_expense": round(service_cost + net_interest + past_service_cost, 2),
        "oci_actuarial_gain_loss": round(actuarial_gain_loss, 2),
        "balance_sheet_entry": {
            "defined_benefit_asset": round(net_asset, 2),
            "defined_benefit_liability": round(net_liability, 2),
        },
    }
