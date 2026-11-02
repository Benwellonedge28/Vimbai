"""Standard costing figures.

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


def material(standard_price_per_unit, standard_quantity):
    """Standard material cost."""
    answer = _call(
        "standard_material_json",
        {
            "standard_price_per_unit": standard_price_per_unit,
            "standard_quantity": standard_quantity,
        },
    )
    if answer is not None:
        return answer
    return {
        "standard_price_per_unit": standard_price_per_unit,
        "standard_quantity": standard_quantity,
        "standard_material_cost": standard_price_per_unit * standard_quantity,
    }


def labour(standard_rate_per_hour, standard_hours):
    """Standard labour cost."""
    answer = _call(
        "standard_labour_json",
        {"standard_rate_per_hour": standard_rate_per_hour, "standard_hours": standard_hours},
    )
    if answer is not None:
        return answer
    return {
        "standard_rate_per_hour": standard_rate_per_hour,
        "standard_hours": standard_hours,
        "standard_labour_cost": standard_rate_per_hour * standard_hours,
    }


def overhead(overhead_rate, standard_activity_level):
    """Standard overhead cost."""
    answer = _call(
        "standard_overhead_json",
        {"overhead_rate": overhead_rate, "standard_activity_level": standard_activity_level},
    )
    if answer is not None:
        return answer
    return {
        "overhead_rate": overhead_rate,
        "standard_activity_level": standard_activity_level,
        "standard_overhead_cost": overhead_rate * standard_activity_level,
    }


def total(standard_direct_material, standard_direct_labour, standard_overhead):
    """Total standard cost."""
    answer = _call(
        "standard_total_json",
        {
            "standard_direct_material": standard_direct_material,
            "standard_direct_labour": standard_direct_labour,
            "standard_overhead": standard_overhead,
        },
    )
    if answer is not None:
        return answer
    return {
        "standard_direct_material": standard_direct_material,
        "standard_direct_labour": standard_direct_labour,
        "standard_overhead": standard_overhead,
        "total_standard_cost": standard_direct_material + standard_direct_labour + standard_overhead,
    }


def per_unit(standard_material_cost, standard_labour_cost, standard_overhead, units_produced):
    """Standard cost per unit."""
    answer = _call(
        "standard_per_unit_json",
        {
            "standard_material_cost": standard_material_cost,
            "standard_labour_cost": standard_labour_cost,
            "standard_overhead": standard_overhead,
            "units_produced": units_produced,
        },
    )
    if answer is not None:
        return answer
    total_standard = standard_material_cost + standard_labour_cost + standard_overhead
    cost_per_unit = total_standard / units_produced if units_produced > 0 else 0
    return {
        "standard_material_cost": standard_material_cost,
        "standard_labour_cost": standard_labour_cost,
        "standard_overhead": standard_overhead,
        "total_standard_cost": total_standard,
        "units_produced": units_produced,
        "standard_cost_per_unit": round(cost_per_unit, 2),
    }


def variance(standard_cost, actual_cost):
    """Standard versus actual cost variance."""
    answer = _call(
        "standard_variance_json",
        {"standard_cost": standard_cost, "actual_cost": actual_cost},
    )
    if answer is not None:
        return answer
    difference = standard_cost - actual_cost
    if difference > 0:
        interpretation = "Favorable"
    elif difference < 0:
        interpretation = "Adverse"
    else:
        interpretation = "None"
    return {
        "standard_cost": standard_cost,
        "actual_cost": actual_cost,
        "variance": difference,
        "interpretation": interpretation,
    }
