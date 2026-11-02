"""Prime cost calculations.

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


def _breakdown(direct_materials, direct_labor, direct_expenses, prime_cost):
    return {
        "direct_materials": direct_materials,
        "direct_labor": direct_labor,
        "direct_expenses": direct_expenses,
        "prime_cost": prime_cost,
    }


def prime_cost(product_id, period, direct_materials, direct_labor, direct_expenses):
    """Prime cost for a product from its direct components."""
    answer = _call(
        "prime_cost_json",
        {
            "product_id": product_id,
            "period": period,
            "direct_materials": direct_materials,
            "direct_labor": direct_labor,
            "direct_expenses": direct_expenses,
        },
    )
    if answer is not None:
        return {
            "direct_materials": answer["direct_materials"],
            "direct_labor": answer["direct_labor"],
            "direct_expenses": answer["direct_expenses"],
            "prime_cost": answer["prime_cost"],
            "cost_breakdown": _breakdown(
                answer["direct_materials"],
                answer["direct_labor"],
                answer["direct_expenses"],
                answer["prime_cost"],
            ),
        }

    total = direct_materials + direct_labor + direct_expenses
    return {
        "direct_materials": direct_materials,
        "direct_labor": direct_labor,
        "direct_expenses": direct_expenses,
        "prime_cost": total,
        "cost_breakdown": _breakdown(direct_materials, direct_labor, direct_expenses, total),
    }


def prime_cost_from_items(product_id, period, items):
    """Prime cost summed from the direct cost items recorded for a product."""
    payload = {
        "product_id": product_id,
        "period": period,
        "items": [
            {
                "item_name": item.item_name,
                "item_type": item.item_type,
                "amount": item.amount,
                "units": item.units,
                "product_id": item.product_id,
            }
            for item in items
        ],
    }
    answer = _call("prime_cost_from_items_json", payload)
    if answer is not None:
        return {
            "direct_materials": answer["direct_materials"],
            "direct_labor": answer["direct_labor"],
            "direct_expenses": answer["direct_expenses"],
            "prime_cost": answer["prime_cost"],
            "cost_breakdown": _breakdown(
                answer["direct_materials"],
                answer["direct_labor"],
                answer["direct_expenses"],
                answer["prime_cost"],
            ),
        }

    mine = [i for i in items if i.product_id == product_id]
    direct_materials = sum(i.amount for i in mine if i.item_type == "direct_material")
    direct_labor = sum(i.amount for i in mine if i.item_type == "direct_labor")
    direct_expenses = sum(i.amount for i in mine if i.item_type == "direct_expense")
    total = direct_materials + direct_labor + direct_expenses
    return {
        "direct_materials": direct_materials,
        "direct_labor": direct_labor,
        "direct_expenses": direct_expenses,
        "prime_cost": total,
        "cost_breakdown": _breakdown(direct_materials, direct_labor, direct_expenses, total),
    }
