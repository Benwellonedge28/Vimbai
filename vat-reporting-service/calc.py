"""VAT return calculations.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("vat_return_json",)


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


def prepare_return(company_id, tax_period, input_transactions, output_transactions):
    """Output VAT, input VAT, the net position and the sales split.

    Standard, zero-rated and exempt sales are separated exactly as the return
    always did; anything that is not "standard" or "zero_rated" counts as exempt.
    """
    payload = {
        "company_id": company_id,
        "tax_period": tax_period,
        "input_transactions": list(input_transactions),
        "output_transactions": list(output_transactions),
    }
    answer = _call("vat_return_json", payload)
    if answer is not None:
        return answer

    total_output_vat = 0.0
    total_input_vat = 0.0
    std_sales = 0.0
    zero_sales = 0.0
    exempt_sales = 0.0
    std_purchases = 0.0

    for tx in output_transactions:
        total_output_vat += tx["amount"] * tx["vat_rate"]
        if tx["transaction_type"] == "standard":
            std_sales += tx["amount"]
        elif tx["transaction_type"] == "zero_rated":
            zero_sales += tx["amount"]
        else:
            exempt_sales += tx["amount"]

    for tx in input_transactions:
        total_input_vat += tx["amount"] * tx["vat_rate"]
        if tx["transaction_type"] == "standard":
            std_purchases += tx["amount"]

    net = total_output_vat - total_input_vat
    payable = max(net, 0)
    refundable = max(-net, 0)
    return {
        "company_id": company_id,
        "tax_period": tax_period,
        "total_output_vat": round(total_output_vat, 2),
        "total_input_vat": round(total_input_vat, 2),
        "net_vat_payable": round(payable, 2),
        "net_vat_refundable": round(refundable, 2),
        "standard_rated_sales": round(std_sales, 2),
        "zero_rated_sales": round(zero_sales, 2),
        "exempt_sales": round(exempt_sales, 2),
        "standard_rated_purchases": round(std_purchases, 2),
        "vat_due_to_authority": round(payable, 2),
    }
