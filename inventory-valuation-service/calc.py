"""Inventory valuation: FIFO, LIFO and weighted average.

The figures come from the Rust library when its extension is installed; the Python
maths stays here as the reference implementation and as the fallback for images
built without the wheel. tests/test_assurance_seam.py holds the two to the same
answers.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os

REQUIRED = ("inventory_valuation_json",)


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    """The extension, but only when it carries the functions this service needs.

    An older wheel on the image imports cleanly and then fails at the first call,
    so the probe is the functions themselves rather than the module.
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


def inventory(company_id, method, purchases, sales, opening_inventory, opening_qty):
    """Cost of goods sold and closing stock under FIFO, LIFO or weighted average.

    FIFO draws the opening stock down pro rata before the purchase layers; LIFO runs
    the layers backwards and only then takes the opening stock, priced at the average
    rather than its own layer cost. Weighted average ignores the order entirely.
    """
    payload = {
        "company_id": company_id,
        "method": method,
        "purchases": list(purchases),
        "sales": list(sales),
        "opening_inventory": opening_inventory,
        "opening_qty": opening_qty,
    }
    answer = _call("inventory_valuation_json", payload)
    if answer is not None:
        return answer

    sorted_purchases = sorted(purchases, key=lambda p: p["date"])
    layers = [(p["date"], p["quantity"], p["unit_cost"]) for p in sorted_purchases]
    total_units = opening_qty + sum(p["quantity"] for p in sorted_purchases)
    total_cost = opening_inventory + sum(p["quantity"] * p["unit_cost"] for p in sorted_purchases)
    units_sold = sum(s["quantity"] for s in sales)
    sales_revenue = sum(s["quantity"] * s["unit_price"] for s in sales)

    remaining_qty = opening_qty
    if method == "fifo":
        remaining_units = units_sold
        cogs = 0.0
        remaining_cost = opening_inventory
        if remaining_units > 0 and remaining_qty > 0:
            taken = min(remaining_units, remaining_qty)
            cogs += remaining_cost * (taken / remaining_qty)
            remaining_cost = remaining_cost * (1 - taken / remaining_qty)
            remaining_qty -= taken
            remaining_units -= taken
        for _, qty, cost in layers:
            if remaining_units <= 0:
                break
            taken = min(remaining_units, qty)
            cogs += taken * cost
            remaining_units -= taken
        ending_qty = total_units - units_sold
        ending_inv = total_cost - cogs
    elif method == "lifo":
        remaining_units = units_sold
        cogs = 0.0
        for _, qty, cost in reversed(layers):
            if remaining_units <= 0:
                break
            taken = min(remaining_units, qty)
            cogs += taken * cost
            remaining_units -= taken
        if remaining_units > 0 and remaining_qty > 0:
            taken = min(remaining_units, remaining_qty)
            cogs += taken * (opening_inventory / remaining_qty)
        ending_qty = total_units - units_sold
        ending_inv = total_cost - cogs
    else:
        avg_cost = total_cost / total_units if total_units else 0
        cogs = avg_cost * units_sold
        ending_qty = total_units - units_sold
        ending_inv = avg_cost * ending_qty

    gross_profit = sales_revenue - cogs
    return {
        "company_id": company_id,
        "method": method,
        "total_goods_available": round(total_cost, 2),
        "total_units_available": total_units,
        "total_units_sold": units_sold,
        "cost_of_goods_sold": round(cogs, 2),
        "ending_inventory": round(ending_inv, 2),
        "ending_qty": ending_qty,
        "total_sales_revenue": round(sales_revenue, 2),
        "gross_profit": round(gross_profit, 2),
        "gross_margin": round(gross_profit / sales_revenue * 100 if sales_revenue else 0, 2),
    }
