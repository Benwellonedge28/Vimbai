"""Absorption costing calculations.

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
    """Ask the extension for one figure set, or None when it is not installed."""
    native = _extension()
    if native is None:
        return None
    return json.loads(getattr(native, name)(json.dumps(payload)))


def product_cost(
    product_id: str,
    product_name: str,
    period: str,
    direct_materials: float,
    direct_labor: float,
    direct_expenses: float,
    manufacturing_overhead: float,
    units_produced: float,
    opening_stock: float,
    closing_stock: float,
) -> dict:
    """Prime cost, total production cost and cost per unit.

    Cost per unit is the raw quotient, unrounded, as the service has always
    reported it.
    """
    payload = {
        "product_id": product_id,
        "product_name": product_name,
        "period": period,
        "direct_materials": direct_materials,
        "direct_labor": direct_labor,
        "direct_expenses": direct_expenses,
        "manufacturing_overhead": manufacturing_overhead,
        "units_produced": units_produced,
        "opening_stock": opening_stock,
        "closing_stock": closing_stock,
    }
    answer = _call("absorption_product_cost_json", payload)
    if answer is not None:
        return answer

    prime_cost = direct_materials + direct_labor + direct_expenses
    total_production_cost = prime_cost + manufacturing_overhead
    return {
        **payload,
        "prime_cost": prime_cost,
        "total_production_cost": total_production_cost,
        "cost_per_unit": total_production_cost / units_produced if units_produced > 0 else 0,
    }


def absorbed_overhead(overhead_cost: float, absorption_base_units: float) -> dict:
    """The absorption rate and the overhead absorbed over the base."""
    answer = _call(
        "absorption_overhead_json",
        {"overhead_cost": overhead_cost, "absorption_base_units": absorption_base_units},
    )
    if answer is not None:
        return {
            "overhead_absorption_rate": answer["overhead_absorption_rate"],
            "absorbed_overhead": answer["absorbed_overhead"],
        }

    rate = overhead_cost / absorption_base_units if absorption_base_units > 0 else 0
    return {"overhead_absorption_rate": rate, "absorbed_overhead": rate * absorption_base_units}


def cost_plus(product_cost: float, markup_percentage: float) -> dict:
    """Markup amount and selling price."""
    answer = _call(
        "absorption_cost_plus_json",
        {"product_cost": product_cost, "markup_percentage": markup_percentage},
    )
    if answer is not None:
        return {
            "markup_amount": answer["markup_amount"],
            "selling_price": answer["selling_price"],
        }

    markup_amount = product_cost * (markup_percentage / 100)
    return {"markup_amount": markup_amount, "selling_price": product_cost + markup_amount}
