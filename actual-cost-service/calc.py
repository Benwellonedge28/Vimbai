"""Actual costs and budget comparison.

The figures come from the Rust library when its extension is installed; the
Python maths stays here as the reference implementation and as the fallback for
images built without the wheel. tests/test_cost_accounting_seam.py holds the two
to the same answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    try:
        import vimbai_native
    except ImportError:
        return None
    return vimbai_native


def engine_in_use() -> str:
    """Announce the calculator so /health can be read in production."""
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


def material(actual_quantity, actual_price_per_unit):
    """Actual material cost."""
    answer = _call(
        "actual_material_json",
        {"actual_quantity": actual_quantity, "actual_price_per_unit": actual_price_per_unit},
    )
    if answer is not None:
        return answer
    return {
        "actual_quantity": actual_quantity,
        "actual_price_per_unit": actual_price_per_unit,
        "actual_material_cost": actual_quantity * actual_price_per_unit,
    }


def labour(actual_hours, actual_rate_per_hour):
    """Actual labour cost."""
    answer = _call(
        "actual_labour_json",
        {"actual_hours": actual_hours, "actual_rate_per_hour": actual_rate_per_hour},
    )
    if answer is not None:
        return answer
    return {
        "actual_hours": actual_hours,
        "actual_rate_per_hour": actual_rate_per_hour,
        "actual_labour_cost": actual_hours * actual_rate_per_hour,
    }


def total(actual_direct_material, actual_direct_labour, actual_overhead):
    """Total actual cost."""
    answer = _call(
        "actual_total_json",
        {
            "actual_direct_material": actual_direct_material,
            "actual_direct_labour": actual_direct_labour,
            "actual_overhead": actual_overhead,
        },
    )
    if answer is not None:
        return answer
    return {
        "actual_direct_material": actual_direct_material,
        "actual_direct_labour": actual_direct_labour,
        "actual_overhead": actual_overhead,
        "total_actual_cost": actual_direct_material + actual_direct_labour + actual_overhead,
    }


def compare_to_budget(actual_cost, budgeted_cost):
    """Actual versus budget."""
    answer = _call(
        "actual_compare_budget_json",
        {"actual_cost": actual_cost, "budgeted_cost": budgeted_cost},
    )
    if answer is not None:
        return answer
    variance = budgeted_cost - actual_cost
    if variance > 0:
        status = "Under budget"
    elif variance < 0:
        status = "Over budget"
    else:
        status = "On budget"
    return {
        "actual_cost": actual_cost,
        "budgeted_cost": budgeted_cost,
        "variance": variance,
        "variance_percent": ((variance / budgeted_cost) * 100) if budgeted_cost != 0 else 0,
        "status": status,
    }


def variance(
    actual_material,
    budgeted_material,
    actual_labour,
    budgeted_labour,
    actual_overhead,
    budgeted_overhead,
):
    """Variance by cost category."""
    answer = _call(
        "actual_variance_json",
        {
            "actual_material": actual_material,
            "budgeted_material": budgeted_material,
            "actual_labour": actual_labour,
            "budgeted_labour": budgeted_labour,
            "actual_overhead": actual_overhead,
            "budgeted_overhead": budgeted_overhead,
        },
    )
    if answer is not None:
        return answer
    material_variance = budgeted_material - actual_material
    labour_variance = budgeted_labour - actual_labour
    overhead_variance = budgeted_overhead - actual_overhead
    return {
        "material": {
            "actual": actual_material,
            "budgeted": budgeted_material,
            "variance": material_variance,
        },
        "labour": {"actual": actual_labour, "budgeted": budgeted_labour, "variance": labour_variance},
        "overhead": {
            "actual": actual_overhead,
            "budgeted": budgeted_overhead,
            "variance": overhead_variance,
        },
        "total_variance": material_variance + labour_variance + overhead_variance,
    }
