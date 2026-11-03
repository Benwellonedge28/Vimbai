"""Tax provision: current and deferred tax.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("tax_provision_json",)


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


def tax_provision(
    company_id,
    fiscal_year,
    pre_tax_income,
    statutory_rate,
    permanent_differences,
    temp_differences,
    loss_carryforward,
    valuation_allowance,
):
    """Current and deferred tax, the split of the deferred balance, and the effective rate.

    Taxable differences add to taxable income and deductible ones subtract; losses
    are used only against positive income, and the effective rate is measured
    against pre-tax income rather than the adjusted figure.
    """
    payload = {
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "pre_tax_income": pre_tax_income,
        "statutory_rate": statutory_rate,
        "permanent_differences": permanent_differences,
        "temp_differences": list(temp_differences),
        "loss_carryforward": loss_carryforward,
        "valuation_allowance": valuation_allowance,
    }
    answer = _call("tax_provision_json", payload)
    if answer is not None:
        return answer

    taxable_income = pre_tax_income + permanent_differences
    for td in temp_differences:
        diff = td["book_amount"] - td["tax_amount"]
        taxable_income += diff if td["type"] == "taxable" else -diff

    if loss_carryforward > 0:
        utilized = min(loss_carryforward, max(taxable_income, 0))
        taxable_income -= utilized
    else:
        utilized = 0

    current_tax = max(taxable_income, 0) * statutory_rate

    dta = 0.0
    dtl = 0.0
    for td in temp_differences:
        diff = abs(td["book_amount"] - td["tax_amount"])
        if td["type"] == "deductible":
            dta += diff * statutory_rate
        else:
            dtl += diff * statutory_rate

    net_deferred = dta - dtl - valuation_allowance
    deferred_tax = dtl - dta
    total_tax = current_tax + deferred_tax
    return {
        "company_id": company_id,
        "fiscal_year": fiscal_year,
        "current_tax_expense": round(current_tax, 2),
        "deferred_tax_expense": round(deferred_tax, 2),
        "total_tax_expense": round(total_tax, 2),
        "effective_rate": round(total_tax / pre_tax_income if pre_tax_income else 0, 4),
        "deferred_tax_assets": round(dta, 2),
        "deferred_tax_liabilities": round(dtl, 2),
        "net_deferred_tax": round(net_deferred, 2),
        "valuation_allowance": valuation_allowance,
        "loss_carryforward_utilized": round(utilized, 2),
        "statutory_rate": statutory_rate,
    }
