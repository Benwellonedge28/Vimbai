"""Total production cost.

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


def calculate(
    product_id,
    department_id,
    period,
    direct_materials,
    direct_labor,
    direct_expenses,
    factory_rent,
    factory_depreciation,
    factory_insurance,
    factory_maintenance,
    other_overhead,
    wip_opening,
    wip_closing,
    units_produced,
):
    """Prime cost, factory overhead, WIP adjustment and cost per unit."""
    answer = _call(
        "production_total_cost_json",
        {
            "product_id": product_id,
            "department_id": department_id,
            "period": period,
            "direct_materials": direct_materials,
            "direct_labor": direct_labor,
            "direct_expenses": direct_expenses,
            "factory_rent": factory_rent,
            "factory_depreciation": factory_depreciation,
            "factory_insurance": factory_insurance,
            "factory_maintenance": factory_maintenance,
            "other_overhead": other_overhead,
            "wip_opening": wip_opening,
            "wip_closing": wip_closing,
            "units_produced": units_produced,
        },
    )
    if answer is not None:
        return answer

    prime_cost = direct_materials + direct_labor + direct_expenses
    total_overhead = factory_rent + factory_depreciation + factory_insurance + factory_maintenance + other_overhead
    total_production_cost = prime_cost + total_overhead + wip_opening - wip_closing
    return {
        "product_id": product_id,
        "department_id": department_id,
        "period": period,
        "direct_materials": direct_materials,
        "direct_labor": direct_labor,
        "direct_expenses": direct_expenses,
        "factory_rent": factory_rent,
        "factory_depreciation": factory_depreciation,
        "factory_insurance": factory_insurance,
        "factory_maintenance": factory_maintenance,
        "other_overhead": other_overhead,
        "wip_opening": wip_opening,
        "wip_closing": wip_closing,
        "units_produced": units_produced,
        "prime_cost": prime_cost,
        "total_overhead": total_overhead,
        "total_production_cost": total_production_cost,
        "cost_per_unit": (total_production_cost / units_produced if units_produced > 0 else 0),
    }
