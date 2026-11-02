"""Activity-based costing.

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


def calculate(company_id, activities, products):
    """Cost the products across unit, batch and product level activities."""
    answer = _call(
        "abc_calculate_json",
        {"company_id": company_id, "activities": activities, "products": products},
    )
    if answer is not None:
        return answer

    def rate(activity):
        return activity["cost_pool"] / activity["driver_volume"] if activity["driver_volume"] else 0

    activity_rates = [
        {
            "activity_id": activity["activity_id"],
            "activity_name": activity["activity_name"],
            "activity_type": activity["activity_type"],
            "rate_per_driver": round(rate(activity), 2),
        }
        for activity in activities
    ]

    product_costs = []
    total_overhead = 0

    for product in products:
        unit_cost = sum(
            product["unit_level_drivers"].get(a["activity_id"], 0) * rate(a)
            for a in activities
            if a["activity_type"] == "unit_level"
        )
        batch_cost = sum(
            product["batch_level_drivers"].get(a["activity_id"], 0) * rate(a)
            for a in activities
            if a["activity_type"] == "batch_level"
        )
        product_level_cost = sum(product["product_level_activities"].values())
        product_cost = unit_cost + batch_cost + product_level_cost
        total_overhead += product_cost

        product_costs.append(
            {
                "product_id": product["product_id"],
                "product_name": product["product_name"],
                "unit_level_cost": round(unit_cost, 2),
                "batch_level_cost": round(batch_cost, 2),
                "product_level_cost": round(product_level_cost, 2),
                "total_product_cost": round(product_cost, 2),
            }
        )

    return {
        "company_id": company_id,
        "activity_rates": activity_rates,
        "product_costs": product_costs,
        "unit_costs": {p["product_id"]: round(sum(p["unit_level_drivers"].values()), 2) for p in products},
        "total_overhead_allocated": total_overhead,
    }
