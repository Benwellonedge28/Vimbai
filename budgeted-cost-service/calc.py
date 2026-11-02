"""Budgeted costs and flexible budgets.

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


def create(department, period, direct_material, direct_labour, overhead, other_costs):
    """Budgeted cost breakdown."""
    answer = _call(
        "budget_create_json",
        {
            "department": department,
            "period": period,
            "direct_material": direct_material,
            "direct_labour": direct_labour,
            "overhead": overhead,
            "other_costs": other_costs,
        },
    )
    if answer is not None:
        return answer
    return {
        "department": department,
        "period": period,
        "direct_material": direct_material,
        "direct_labour": direct_labour,
        "overhead": overhead,
        "other_costs": other_costs,
        "total_budgeted_cost": direct_material + direct_labour + overhead + other_costs,
    }


def flexible(budgeted_cost_per_unit, actual_output, budgeted_output):
    """Flexed budget and volume variance."""
    answer = _call(
        "budget_flexible_json",
        {
            "budgeted_cost_per_unit": budgeted_cost_per_unit,
            "actual_output": actual_output,
            "budgeted_output": budgeted_output,
        },
    )
    if answer is not None:
        return answer
    flexed = budgeted_cost_per_unit * actual_output
    original = budgeted_cost_per_unit * budgeted_output
    return {
        "budgeted_cost_per_unit": budgeted_cost_per_unit,
        "budgeted_output": budgeted_output,
        "actual_output": actual_output,
        "original_budget": original,
        "flexed_budget": flexed,
        "volume_variance": flexed - original,
    }


def total(budgeted_costs):
    """Total of the budgeted cost items."""
    answer = _call("budget_total_json", {"budgeted_costs": budgeted_costs})
    if answer is not None:
        return answer
    items = []
    running = 0
    for item in budgeted_costs:
        amount = item.get("amount", 0)
        running += amount
        items.append({"item": item.get("item", ""), "amount": amount})
    return {"budget_items": items, "total_budgeted_cost": running}


def compare_output(budgeted_units, actual_units, budgeted_cost_per_unit):
    """Budgeted versus flexed output cost."""
    answer = _call(
        "budget_compare_output_json",
        {
            "budgeted_units": budgeted_units,
            "actual_units": actual_units,
            "budgeted_cost_per_unit": budgeted_cost_per_unit,
        },
    )
    if answer is not None:
        return answer
    budgeted_total = budgeted_units * budgeted_cost_per_unit
    flexed = actual_units * budgeted_cost_per_unit
    return {
        "budgeted_units": budgeted_units,
        "actual_units": actual_units,
        "budgeted_cost_per_unit": budgeted_cost_per_unit,
        "budgeted_total_cost": budgeted_total,
        "flexed_budget_cost": flexed,
        "output_variance": budgeted_total - flexed,
    }


def breakdown(total_budgeted_cost, material_percent, labour_percent, overhead_percent):
    """Split a budget by cost category."""
    answer = _call(
        "budget_breakdown_json",
        {
            "total_budgeted_cost": total_budgeted_cost,
            "material_percent": material_percent,
            "labour_percent": labour_percent,
            "overhead_percent": overhead_percent,
        },
    )
    if answer is not None:
        return answer
    total_percent = material_percent + labour_percent + overhead_percent
    if abs(total_percent - 100) > 0.01:
        return {"error": "Percentages must sum to 100"}
    return {
        "total_budgeted_cost": total_budgeted_cost,
        "material": total_budgeted_cost * (material_percent / 100),
        "labour": total_budgeted_cost * (labour_percent / 100),
        "overhead": total_budgeted_cost * (overhead_percent / 100),
    }
