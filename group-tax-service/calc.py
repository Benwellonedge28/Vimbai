"""Group tax consolidation.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("group_tax_json",)


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


def consolidate(group_id, fiscal_year, subsidiaries, group_tax_rate, intercompany_eliminations):
    """Group income, the tax on it, and what consolidation saved.

    Losses in one entity shelter profit in another before the group rate is applied;
    savings are measured against each entity's standalone charge net of what it
    already paid, and never reported as negative.
    """
    payload = {
        "group_id": group_id,
        "fiscal_year": fiscal_year,
        "subsidiaries": list(subsidiaries),
        "group_tax_rate": group_tax_rate,
        "intercompany_eliminations": intercompany_eliminations,
    }
    answer = _call("group_tax_json", payload)
    if answer is not None:
        return answer

    total_income = sum(s["pre_tax_income"] for s in subsidiaries) - intercompany_eliminations
    consolidated_income = max(total_income, 0)
    consolidated_tax = consolidated_income * group_tax_rate

    standalone_tax = sum(max(s["pre_tax_income"], 0) * s["tax_rate"] for s in subsidiaries)
    savings = (
        standalone_tax
        - consolidated_tax
        - sum(
            max(s["pre_tax_income"], 0) * s["tax_rate"] - s["tax_paid"] for s in subsidiaries if s["pre_tax_income"] > 0
        )
    )
    return {
        "group_id": group_id,
        "fiscal_year": fiscal_year,
        "consolidated_income": round(consolidated_income, 2),
        "consolidated_tax": round(consolidated_tax, 2),
        "total_tax_already_paid": round(sum(s["tax_paid"] for s in subsidiaries), 2),
        "net_group_tax": round(max(consolidated_tax - sum(s["tax_paid"] for s in subsidiaries), 0), 2),
        "tax_savings_from_consolidation": round(max(savings, 0), 2),
        "subsidiary_summary": [
            {
                "entity_id": s["entity_id"],
                "name": s["entity_name"],
                "jurisdiction": s["jurisdiction"],
                "income": s["pre_tax_income"],
                "tax_paid": s["tax_paid"],
                "rate": s["tax_rate"],
                "loss_carryforward": s["loss_carryforward"],
            }
            for s in subsidiaries
        ],
    }
