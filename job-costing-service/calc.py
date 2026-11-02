"""Job cost rollups.

The rollup comes from the Rust library when its extension is installed; the
Python maths stays here as the reference implementation and as the fallback for
images built without the wheel. tests/test_cost_accounting_seam.py holds the two
to the same answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

# Only these four types roll up; anything else is recorded but not aggregated.
ROLLUP_TYPES = ("materials", "labor", "overhead", "subcontractor")


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    try:
        import vimbai_native
    except ImportError:
        return None
    return vimbai_native


def engine_in_use() -> str:
    """Announce the calculator so the health endpoint can be read in production."""
    requested = os.getenv("VIMBAI_CALC_ENGINE", "auto").strip().lower()
    if requested in ("python", "reference"):
        return "python"
    if _extension() is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is not installed")
        return "python"
    return "rust"


def _call(name, payload):
    native = _extension()
    if native is None:
        return None
    return json.loads(getattr(native, name)(json.dumps(payload)))


def apply_cost(
    job,
    cost_type,
    amount,
    contract_value,
    total_cost,
    gross_profit,
    gross_margin,
):
    """Apply one cost entry to the job's running totals and rollup.

    ``job`` is any object with the four cost attributes; the returned values are
    plain numbers the caller writes back.
    """
    components = {
        "materials_cost": job.materials_cost,
        "labor_cost": job.labor_cost,
        "overhead_cost": job.overhead_cost,
        "subcontractor_cost": job.subcontractor_cost,
    }
    answer = _call(
        "job_add_cost_json",
        {
            **components,
            "cost_type": cost_type,
            "amount": amount,
            "contract_value": contract_value,
            "total_cost": total_cost,
            "gross_profit": gross_profit,
            "gross_margin": gross_margin,
        },
    )
    if answer is not None:
        return answer

    if cost_type in ROLLUP_TYPES:
        components[f"{cost_type}_cost"] = components[f"{cost_type}_cost"] + amount
        total = (
            components["materials_cost"]
            + components["labor_cost"]
            + components["overhead_cost"]
            + components["subcontractor_cost"]
        )
        profit = contract_value - total
        return {
            "aggregated": True,
            **components,
            "total_cost": total,
            "gross_profit": profit,
            "gross_margin": (profit / max(1, contract_value)) * 100,
        }

    return {
        "aggregated": False,
        **components,
        "total_cost": total_cost,
        "gross_profit": gross_profit,
        "gross_margin": gross_margin,
    }
