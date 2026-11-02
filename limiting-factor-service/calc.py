"""Limiting factor (scarce resource) calculations.

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


def analyze(limiting_factor, factor_type, total_available, products):
    """Optimal production plan: rank by contribution per factor unit, allocate greedily."""
    answer = _call(
        "limiting_factor_analyze_json",
        {
            "limiting_factor": limiting_factor,
            "factor_type": factor_type,
            "total_available": total_available,
            "products": products,
        },
    )
    if answer is not None:
        return answer

    ranked = []
    for p in products:
        contribution_per_factor_unit = 0
        if p["factor_per_unit"] > 0:
            contribution_per_factor_unit = p["contribution_per_unit"] / p["factor_per_unit"]
        ranked.append(
            {
                "product_id": p["product_id"],
                "product_name": p["product_name"],
                "demand": p["demand"],
                "contribution_per_unit": p["contribution_per_unit"],
                "factor_per_unit": p["factor_per_unit"],
                "contribution_per_factor_unit": contribution_per_factor_unit,
                "ranking": 0,
                "units_to_produce": 0,
                "total_contribution": 0,
            }
        )

    ranked.sort(key=lambda x: x["contribution_per_factor_unit"], reverse=True)

    remaining = total_available
    factor_used = 0
    total_contribution = 0
    plan = {}

    for i, product in enumerate(ranked, 1):
        product["ranking"] = i
        if product["factor_per_unit"] > 0:
            product["units_to_produce"] = min(product["demand"], remaining / product["factor_per_unit"])
        else:
            product["units_to_produce"] = product["demand"]

        used = product["units_to_produce"] * product["factor_per_unit"]
        remaining -= used
        factor_used += used
        product["total_contribution"] = product["units_to_produce"] * product["contribution_per_unit"]
        total_contribution += product["total_contribution"]
        plan[product["product_id"]] = product["units_to_produce"]

    return {
        "limiting_factor": limiting_factor,
        "factor_type": factor_type,
        "total_available": total_available,
        "factor_used": factor_used,
        "factor_remaining": remaining,
        "total_contribution": total_contribution,
        "products": ranked,
        "optimal_production_plan": plan,
    }


def compare(products):
    """Products ranked by contribution per unit of the scarce resource."""
    answer = _call("limiting_factor_compare_json", {"products": products})
    if answer is not None:
        return answer["ranked_products"]

    comparisons = []
    for p in products:
        contribution_per_factor = 0
        if p["factor_per_unit"] > 0:
            contribution_per_factor = p["contribution_per_unit"] / p["factor_per_unit"]
        comparisons.append(
            {
                "product_id": p["product_id"],
                "product_name": p["product_name"],
                "contribution_per_unit": p["contribution_per_unit"],
                "factor_per_unit": p["factor_per_unit"],
                "contribution_per_factor_unit": contribution_per_factor,
            }
        )

    comparisons.sort(key=lambda x: x["contribution_per_factor_unit"], reverse=True)
    for i, comp in enumerate(comparisons, 1):
        comp["ranking"] = i
    return comparisons
